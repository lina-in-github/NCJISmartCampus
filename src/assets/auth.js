/* 智慧校园 · 非对称登录客户端
 *
 * 正常登录时密码不离开浏览器：用 PBKDF2-HMAC-SHA256 派生 X25519 私钥，
 * 公钥在注册 / 重置密码时上传；登录时只需用私钥解密服务器挑战。
 * 旧账号（只有密码哈希、没有公钥）首次登录会通过 /api/upgrade-pubkey
 * 做一次公钥迁移，之后不再发送密码。
 *
 * 该文件同时暴露 window.SCAuthCrypto，便于 Node 端测试与服务端文档示例。
 */
(function (root) {
  "use strict";

  var ITERATIONS = 210000;
  var SALT_PREFIX = "smart-campus/auth/v1:";
  var CHALLENGE_INFO = "smart-campus/challenge/v1";
  var KEY_LEN = 32;
  var NONCE_LEN = 12;
  var TAG_LEN = 16;
  var CHALLENGE_LEN = 32;

  // -------------------------------------------------------------------
  // 编码工具
  // -------------------------------------------------------------------

  function utf8(text) {
    return new TextEncoder().encode(String(text));
  }

  function bytesToB64(bytes) {
    var binary = "";
    for (var i = 0; i < bytes.length; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
  }

  function b64ToBytes(text) {
    var normalized = String(text || "")
      .replace(/\s+/g, "")
      .replace(/-/g, "+")
      .replace(/_/g, "/");
    while (normalized.length % 4 !== 0) normalized += "=";
    var binary = atob(normalized);
    var out = new Uint8Array(binary.length);
    for (var i = 0; i < binary.length; i++) {
      out[i] = binary.charCodeAt(i);
    }
    return out;
  }

  function concatBytes(parts) {
    var total = 0;
    parts.forEach(function (p) {
      total += p.length;
    });
    var out = new Uint8Array(total);
    var offset = 0;
    parts.forEach(function (p) {
      out.set(p, offset);
      offset += p.length;
    });
    return out;
  }

  function bytesToHex(bytes) {
    var out = "";
    for (var i = 0; i < bytes.length; i++) {
      out += (bytes[i] < 16 ? "0" : "") + bytes[i].toString(16);
    }
    return out;
  }

  function hexToBytes(hex) {
    var text = String(hex || "").replace(/\s+/g, "");
    if (text.length % 2 !== 0) throw new Error("invalid hex");
    var out = new Uint8Array(text.length / 2);
    for (var i = 0; i < out.length; i++) {
      var v = parseInt(text.slice(i * 2, i * 2 + 2), 16);
      if (isNaN(v)) throw new Error("invalid hex");
      out[i] = v;
    }
    return out;
  }

  // -------------------------------------------------------------------
  // X25519（RFC 7748，Montgomery ladder）
  // -------------------------------------------------------------------

  var P = (1n << 255n) - 19n;
  var A24 = 121665n;

  function modP(a) {
    var r = a % P;
    return r >= 0n ? r : r + P;
  }

  function powMod(base, exp) {
    var result = 1n;
    var b = modP(base);
    var e = exp;
    while (e > 0n) {
      if (e & 1n) result = (result * b) % P;
      b = (b * b) % P;
      e >>= 1n;
    }
    return result;
  }

  function invMod(a) {
    return powMod(a, P - 2n);
  }

  function leToBig(bytes) {
    var out = 0n;
    for (var i = bytes.length - 1; i >= 0; i--) {
      out = (out << 8n) | BigInt(bytes[i]);
    }
    return out;
  }

  function bigToLe(value) {
    var out = new Uint8Array(KEY_LEN);
    var n = value;
    for (var i = 0; i < KEY_LEN; i++) {
      out[i] = Number(n & 0xffn);
      n >>= 8n;
    }
    return out;
  }

  function x25519(scalar, u) {
    if (!scalar || scalar.length !== KEY_LEN) {
      throw new Error("invalid X25519 scalar");
    }
    if (!u || u.length !== KEY_LEN) {
      throw new Error("invalid X25519 public key");
    }
    var k = new Uint8Array(scalar);
    k[0] &= 248;
    k[31] &= 127;
    k[31] |= 64;
    var uu = new Uint8Array(u);
    uu[31] &= 127;

    var x1 = modP(leToBig(uu));
    var x2 = 1n;
    var z2 = 0n;
    var x3 = x1;
    var z3 = 1n;
    var swap = 0;

    for (var t = 254; t >= 0; t--) {
      var kt = (k[t >> 3] >> (t & 7)) & 1;
      swap ^= kt;
      if (swap) {
        var tx = x2;
        x2 = x3;
        x3 = tx;
        var tz = z2;
        z2 = z3;
        z3 = tz;
      }
      swap = kt;

      var A = modP(x2 + z2);
      var AA = (A * A) % P;
      var B = modP(x2 - z2);
      var BB = (B * B) % P;
      var E = modP(AA - BB);
      var C = modP(x3 + z3);
      var D = modP(x3 - z3);
      var DA = (D * A) % P;
      var CB = (C * B) % P;
      var DA_plus_CB = modP(DA + CB);
      var DA_minus_CB = modP(DA - CB);
      x3 = (DA_plus_CB * DA_plus_CB) % P;
      z3 = (x1 * ((DA_minus_CB * DA_minus_CB) % P)) % P;
      x2 = (AA * BB) % P;
      z2 = (E * modP(AA + A24 * E)) % P;
    }
    if (swap) {
      var fx = x2;
      x2 = x3;
      x3 = fx;
      var fz = z2;
      z2 = z3;
      z3 = fz;
    }
    return bigToLe(modP(x2 * invMod(z2)));
  }

  function publicKey(privateBytes) {
    var base = new Uint8Array(KEY_LEN);
    base[0] = 9;
    return x25519(privateBytes, base);
  }

  function sharedSecret(privateBytes, peerPublic) {
    return x25519(privateBytes, peerPublic);
  }

  // -------------------------------------------------------------------
  // KDF / AEAD（Web Crypto）
  // -------------------------------------------------------------------

  function subtle() {
    var c = root.crypto || (typeof crypto !== "undefined" ? crypto : null);
    if (!c || !c.subtle) {
      throw new Error("WEB_CRYPTO_UNAVAILABLE");
    }
    return c.subtle;
  }

  function randomBytes(len) {
    var c = root.crypto || (typeof crypto !== "undefined" ? crypto : null);
    if (!c || !c.getRandomValues) {
      throw new Error("WEB_CRYPTO_UNAVAILABLE");
    }
    return c.getRandomValues(new Uint8Array(len));
  }

  function defaultSalt(username) {
    return SALT_PREFIX + String(username || "").trim().toLowerCase();
  }

  async function pbkdf2(password, salt, iterations) {
    var key = await subtle().importKey("raw", utf8(password), "PBKDF2", false, [
      "deriveBits"
    ]);
    var bits = await subtle().deriveBits(
      {
        name: "PBKDF2",
        hash: "SHA-256",
        salt: utf8(salt),
        iterations: iterations || ITERATIONS
      },
      key,
      KEY_LEN * 8
    );
    return new Uint8Array(bits);
  }

  async function hkdf(ikm, salt, info) {
    var key = await subtle().importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
    var bits = await subtle().deriveBits(
      {
        name: "HKDF",
        hash: "SHA-256",
        salt: salt || new Uint8Array(0),
        info: utf8(info)
      },
      key,
      KEY_LEN * 8
    );
    return new Uint8Array(bits);
  }

  async function aesGcmKey(rawKey, usage) {
    return subtle().importKey("raw", rawKey, "AES-GCM", false, [usage]);
  }

  async function aesGcmEncrypt(rawKey, nonce, plaintext) {
    var key = await aesGcmKey(rawKey, "encrypt");
    var ciphertext = await subtle().encrypt(
      { name: "AES-GCM", iv: nonce, tagLength: TAG_LEN * 8 },
      key,
      plaintext
    );
    return new Uint8Array(ciphertext);
  }

  async function aesGcmDecrypt(rawKey, nonce, ciphertext) {
    var key = await aesGcmKey(rawKey, "decrypt");
    var plaintext = await subtle().decrypt(
      { name: "AES-GCM", iv: nonce, tagLength: TAG_LEN * 8 },
      key,
      ciphertext
    );
    return new Uint8Array(plaintext);
  }

  /** 从密码派生 X25519 私钥。salt 可直接使用 logintry 返回的 kdf.salt。 */
  async function derivePrivateKey(password, salt, iterations) {
    if (!password) throw new Error("PASSWORD_REQUIRED");
    if (!salt) throw new Error("SALT_REQUIRED");
    return pbkdf2(password, salt, iterations);
  }

  /** 解密 logintry 返回的 challengeEncrypt，得到 32 字节挑战。 */
  async function decryptChallenge(privateBytes, challengeEncryptB64) {
    var raw = b64ToBytes(challengeEncryptB64);
    if (raw.length !== KEY_LEN + NONCE_LEN + CHALLENGE_LEN + TAG_LEN) {
      throw new Error("INVALID_CHALLENGE");
    }
    var ephemeralPub = raw.slice(0, KEY_LEN);
    var nonce = raw.slice(KEY_LEN, KEY_LEN + NONCE_LEN);
    var ciphertext = raw.slice(KEY_LEN + NONCE_LEN);
    var shared = sharedSecret(privateBytes, ephemeralPub);
    var key = await hkdf(shared, new Uint8Array(0), CHALLENGE_INFO);
    var challenge;
    try {
      challenge = await aesGcmDecrypt(key, nonce, ciphertext);
    } catch (e) {
      // 最常见原因：密码错误，派生出的私钥无法解开挑战。
      throw new Error("WRONG_PASSWORD");
    }
    if (challenge.length !== CHALLENGE_LEN) throw new Error("INVALID_CHALLENGE");
    return challenge;
  }

  /** 用 logintry 返回的 answerEncryptingKey 加密挑战，返回 base64。 */
  async function encryptAnswer(answerKeyB64, challenge) {
    var key = b64ToBytes(answerKeyB64);
    if (key.length !== KEY_LEN) throw new Error("INVALID_ANSWER_KEY");
    if (!challenge || challenge.length !== CHALLENGE_LEN) {
      throw new Error("INVALID_CHALLENGE");
    }
    var nonce = randomBytes(NONCE_LEN);
    var ciphertext = await aesGcmEncrypt(key, nonce, challenge);
    return bytesToB64(concatBytes([nonce, ciphertext]));
  }

  var api = {
    ITERATIONS: ITERATIONS,
    SALT_PREFIX: SALT_PREFIX,
    CHALLENGE_INFO: CHALLENGE_INFO,
    defaultSalt: defaultSalt,
    derivePrivateKey: derivePrivateKey,
    publicKey: publicKey,
    bytesToB64: bytesToB64,
    b64ToBytes: b64ToBytes,
    bytesToHex: bytesToHex,
    hexToBytes: hexToBytes,
    x25519: x25519,
    decryptChallenge: decryptChallenge,
    encryptAnswer: encryptAnswer,
    pbkdf2: pbkdf2,
    hkdf: hkdf
  };

  root.SCAuthCrypto = api;
  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  }

  // -------------------------------------------------------------------
  // 页面交互：仅在有 DOM 时启用
  // -------------------------------------------------------------------

  if (typeof document === "undefined") return;

  function S(key) {
    var source = root.STR || {};
    return Object.prototype.hasOwnProperty.call(source, key) ? source[key] : key;
  }

  function $(sel, ctx) {
    return (ctx || document).querySelector(sel);
  }

  function setLoading(btn, loading, text) {
    if (!btn) return;
    if (loading) {
      btn.dataset.originalText = btn.innerHTML;
      btn.classList.add("loading");
      btn.disabled = true;
      if (text) btn.textContent = text;
    } else {
      btn.classList.remove("loading");
      btn.disabled = false;
      if (btn.dataset.originalText) btn.innerHTML = btn.dataset.originalText;
      delete btn.dataset.originalText;
    }
  }

  function toast(message, type) {
    var wrap = $(".toast-wrap");
    if (!wrap) {
      wrap = document.createElement("div");
      wrap.className = "toast-wrap";
      document.body.appendChild(wrap);
    }
    var el = document.createElement("div");
    el.className = "toast " + (type === "error" ? "err" : "ok");
    var icon =
      type === "error"
        ? '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 8v5M12 16h.01"/></svg>'
        : '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>';
    el.innerHTML = icon + "<span></span>";
    el.querySelector("span").textContent = message;
    wrap.appendChild(el);
    setTimeout(function () {
      el.classList.add("leaving");
      setTimeout(function () {
        if (el.parentNode) el.parentNode.removeChild(el);
      }, 260);
    }, 3800);
  }

  async function request(path, body) {
    var res = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      credentials: "same-origin",
      body: JSON.stringify(body || {})
    });
    var data = {};
    try {
      data = await res.json();
    } catch (e) {
      /* ignore */
    }
    if (!res.ok || data.ok === false) {
      var err = new Error(data.msg || data.message || "request failed");
      err.status = res.status;
      err.code = data.code || "";
      err.data = data;
      throw err;
    }
    return data;
  }

  function friendlyError(err) {
    if (err && err.message === "WEB_CRYPTO_UNAVAILABLE") {
      return S("flash_crypto_unsupported");
    }
    if (err && err.message === "WRONG_PASSWORD") {
      return S("flash_wrong_credentials");
    }
    if (err && err.message === "INVALID_CHALLENGE") {
      return S("flash_login_challenge_error");
    }
    if (err && err.status === 401) {
      if (err.code === "pubkey_missing") return S("flash_pubkey_missing");
      return S("flash_wrong_credentials");
    }
    if (err && err.status === 400) {
      if (err.code === "username_taken") return S("flash_username_taken");
      var message = String(err.message || "");
      if (message.indexOf("key is already bound") >= 0) return S("flash_key_bound");
      if (message.indexOf("invalid key") >= 0) return S("flash_key_invalid");
      if (message.indexOf("public key") >= 0) return S("flash_pubkey_invalid");
      return S("flash_request_failed");
    }
    if (err && err.status >= 500) return S("flash_server_error");
    return S("flash_request_failed");
  }

  function redirectUser(messageKey) {
    var msg = messageKey ? S(messageKey) : "";
    location.href = msg ? "/user?ok=" + encodeURIComponent(msg) : "/user";
  }

  function field(form, name) {
    var el = form.elements.namedItem(name);
    return el ? String(el.value || "") : "";
  }

  // 密码输入框刻意不带 name，避免 JS 未加载时浏览器把密码 POST 给服务器。
  function valueById(form, id) {
    var el = form.querySelector("#" + id) || document.getElementById(id);
    return el ? String(el.value || "") : "";
  }

  async function performLogin(username, password, button) {
    var info = null;
    var salt = defaultSalt(username);
    var iterations = ITERATIONS;
    try {
      info = await request("/api/logintry", { username: username });
    } catch (err) {
      if (err.code === "pubkey_missing") {
        // 兼容老账号：用旧密码做一次迁移，把公钥交给服务端。
        var legacyPrivate = await derivePrivateKey(password, salt, iterations);
        try {
          await request("/api/upgrade-pubkey", {
            username: username,
            password: password,
            pubkey: bytesToB64(publicKey(legacyPrivate))
          });
        } catch (upgradeError) {
          // 另一个标签页可能已经完成迁移；400 时继续重试 logintry。
          if (upgradeError.status !== 400) throw upgradeError;
        }
        info = await request("/api/logintry", { username: username });
      } else {
        throw err;
      }
    }
    if (info.kdf) {
      salt = info.kdf.salt || salt;
      iterations = info.kdf.iterations || iterations;
    }
    var privateBytes = await derivePrivateKey(password, salt, iterations);
    var challenge = await decryptChallenge(privateBytes, info.challengeEncrypt);
    var encryptedAnswer = await encryptAnswer(info.answerEncryptingKey, challenge);
    var result = await request("/api/getAuths", {
      loginSessionID: info.loginSessionID,
      encryptedAnswer: encryptedAnswer
    });
    redirectUser("flash_welcome");
    return result;
  }

  function wireLoginForm() {
    var form = $("form[data-auth-login]");
    if (!form) return;
    form.addEventListener("submit", async function (e) {
      e.preventDefault();
      var button = form.querySelector("button[type=submit]");
      setLoading(button, true);
      try {
        await performLogin(field(form, "username").trim(), valueById(form, "password"), button);
      } catch (err) {
        if (err && err.message) console.error(err.message);
        toast(friendlyError(err), "error");
      } finally {
        setLoading(button, false);
      }
    });
  }

  function wireRegisterForm() {
    var form = $("form[data-auth-register]");
    if (!form) return;
    form.addEventListener("submit", async function (e) {
      e.preventDefault();
      var button = form.querySelector("button[type=submit]");
      var username = field(form, "username").trim();
      var password = valueById(form, "new-password");
      var password2 = valueById(form, "new-password2");
      if (password.length < 6) {
        toast(S("flash_password_min"), "error");
        return;
      }
      if (password !== password2) {
        toast(S("flash_password_mismatch"), "error");
        return;
      }
      setLoading(button, true);
      try {
        var salt = defaultSalt(username);
        var privateBytes = await derivePrivateKey(password, salt, ITERATIONS);
        await request("/api/register", {
          username: username,
          passwordPubKey: bytesToB64(publicKey(privateBytes)),
          key: field(form, "key").trim()
        });
        await performLogin(username, password, button);
      } catch (err) {
        if (err && err.message) console.error(err.message);
        toast(friendlyError(err), "error");
      } finally {
        setLoading(button, false);
      }
    });
  }

  function wirePasswordForm() {
    var form = $("form[data-auth-password]");
    if (!form) return;
    form.addEventListener("submit", async function (e) {
      e.preventDefault();
      var button = form.querySelector("button[type=submit]");
      var username = form.getAttribute("data-username") || "";
      var oldPassword = valueById(form, "old-pw");
      var newPassword = valueById(form, "new-pw");
      var newPassword2 = valueById(form, "new-pw2");
      if (newPassword.length < 6) {
        toast(S("flash_password_min"), "error");
        return;
      }
      if (newPassword !== newPassword2) {
        toast(S("flash_password_mismatch"), "error");
        return;
      }
      if (!username) {
        toast(S("flash_need_login"), "error");
        return;
      }
      setLoading(button, true);
      try {
        var info = await request("/api/logintry", { username: username });
        var salt = (info.kdf && info.kdf.salt) || defaultSalt(username);
        var iterations = (info.kdf && info.kdf.iterations) || ITERATIONS;
        var oldPrivate = await derivePrivateKey(oldPassword, salt, iterations);
        var challenge = await decryptChallenge(oldPrivate, info.challengeEncrypt);
        var answer = await encryptAnswer(info.answerEncryptingKey, challenge);
        var newPrivate = await derivePrivateKey(newPassword, salt, iterations);
        var newPub = bytesToB64(publicKey(newPrivate));
        await request("/api/resetPassword", {
          loginSessionID: info.loginSessionID,
          encryptedAnswer: answer,
          newPasswordPubKey: newPub
        });
        redirectUser("flash_password_changed");
      } catch (err) {
        if (err && err.message) console.error(err.message);
        toast(friendlyError(err), "error");
      } finally {
        setLoading(button, false);
      }
    });
  }

  wireLoginForm();
  wireRegisterForm();
  wirePasswordForm();
})(typeof window !== "undefined" ? window : globalThis);
