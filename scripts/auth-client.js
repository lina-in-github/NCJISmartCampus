#!/usr/bin/env node
/*
 * 智慧校园 · 非对称登录命令行客户端
 *
 * 这个脚本复用 src/assets/auth.js 中的密码学实现，用于本地测试 / smoke.sh。
 * 浏览器页面会自动完成同样的步骤，普通用户无需运行它。
 *
 * 用法：
 *   node scripts/auth-client.js login  --base http://127.0.0.1:3000 --username student --password student123
 *   node scripts/auth-client.js register --base ... --username alice --password pass1234 [--key SC-STU-...]
 *   node scripts/auth-client.js reset  --base ... --username alice --old-password old --new-password new
 *   node scripts/auth-client.js pubkey --username alice --password pass1234
 *
 * 输出为 JSON：{ ok, auths, user, cookie }；失败时退出码非 0。
 */
"use strict";

const crypto = require("../src/assets/auth.js");

function usage(message) {
  if (message) console.error(message);
  console.error(
    "usage: auth-client.js login|register|reset|pubkey --base URL --username NAME --password PASSWORD"
  );
  process.exit(2);
}

function parseArgs(argv) {
  const args = { base: "http://127.0.0.1:3000", command: "login" };
  const rest = argv.slice(2);
  if (!rest.length) usage("missing command");
  args.command = rest.shift();
  if (!["login", "register", "reset", "pubkey"].includes(args.command)) {
    usage("unknown command: " + args.command);
  }
  for (let i = 0; i < rest.length; i++) {
    const item = rest[i];
    if (!item.startsWith("--")) usage("unexpected argument: " + item);
    const key = item.slice(2);
    const value = rest[i + 1];
    if (value === undefined || value.startsWith("--")) usage("missing value for --" + key);
    args[key] = value;
    i++;
  }
  return args;
}

async function request(base, path, body) {
  const res = await fetch(base.replace(/\/$/, "") + path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body || {})
  });
  let data = {};
  try {
    data = await res.json();
  } catch (e) {
    /* ignore */
  }
  if (!res.ok || data.ok === false) {
    const err = new Error(data.msg || data.message || "HTTP " + res.status);
    err.status = res.status;
    err.code = data.code || "";
    throw err;
  }
  return { data, headers: res.headers };
}

async function challenge(base, username, password) {
  const start = await request(base, "/api/logintry", { username: username });
  const info = start.data;
  const salt = (info.kdf && info.kdf.salt) || crypto.defaultSalt(info.username || username);
  const iterations = (info.kdf && info.kdf.iterations) || crypto.ITERATIONS;
  const privateKey = await crypto.derivePrivateKey(password, salt, iterations);
  const challengeBytes = await crypto.decryptChallenge(privateKey, info.challengeEncrypt);
  const encryptedAnswer = await crypto.encryptAnswer(
    info.answerEncryptingKey,
    challengeBytes
  );
  return { info, salt, iterations, privateKey, challengeBytes, encryptedAnswer };
}

function accountCookie(headers) {
  const all =
    typeof headers.getSetCookie === "function" ? headers.getSetCookie() : [];
  for (const line of all) {
    const match = /(?:^|,\s*)(sc_uid=[^;]+)/.exec(line);
    if (match) return match[1];
  }
  const single = headers.get("set-cookie") || "";
  const match = /(?:^|,\s*)(sc_uid=[^;]+)/.exec(single);
  return match ? match[1] : "";
}

async function doLogin(base, username, password) {
  const { info, encryptedAnswer } = await challenge(base, username, password);
  const got = await request(base, "/api/getAuths", {
    loginSessionID: info.loginSessionID,
    encryptedAnswer
  });
  return {
    ok: true,
    auths: got.data.auths || [],
    members: got.data.members || [],
    user: got.data.user || {},
    cookie: accountCookie(got.headers)
  };
}

async function doRegister(base, username, password, key) {
  const privateKey = await crypto.derivePrivateKey(
    password,
    crypto.defaultSalt(username),
    crypto.ITERATIONS
  );
  const pubkey = crypto.bytesToB64(crypto.publicKey(privateKey));
  const body = { username, passwordPubKey: pubkey };
  if (key) body.key = key;
  await request(base, "/api/register", body);
  return doLogin(base, username, password);
}

async function doReset(base, username, oldPassword, newPassword) {
  const { info, encryptedAnswer, salt, iterations } = await challenge(
    base,
    username,
    oldPassword
  );
  const newPrivate = await crypto.derivePrivateKey(newPassword, salt, iterations);
  await request(base, "/api/resetPassword", {
    loginSessionID: info.loginSessionID,
    encryptedAnswer,
    newPasswordPubKey: crypto.bytesToB64(crypto.publicKey(newPrivate))
  });
  return doLogin(base, username, newPassword);
}

async function main() {
  const args = parseArgs(process.argv);
  const base = args.base;
  const username = args.username;
  if (!username) usage("--username is required");

  let result;
  if (args.command === "pubkey") {
    if (!args.password) usage("--password is required");
    const salt = crypto.defaultSalt(username);
    const privateKey = await crypto.derivePrivateKey(
      args.password,
      salt,
      crypto.ITERATIONS
    );
    result = {
      ok: true,
      username,
      salt,
      pubkey: crypto.bytesToB64(crypto.publicKey(privateKey))
    };
  } else if (args.command === "login") {
    if (!args.password) usage("--password is required");
    result = await doLogin(base, username, args.password);
  } else if (args.command === "register") {
    if (!args.password) usage("--password is required");
    result = await doRegister(base, username, args.password, args.key || "");
  } else {
    if (!args["old-password"] || !args["new-password"]) {
      usage("--old-password and --new-password are required");
    }
    result = await doReset(
      base,
      username,
      args["old-password"],
      args["new-password"]
    );
  }
  process.stdout.write(JSON.stringify(result) + "\n");
}

main().catch((err) => {
  process.stderr.write(
    JSON.stringify({ ok: false, status: err.status || 0, code: err.code || "", msg: err.message }) +
      "\n"
  );
  process.exit(1);
});
