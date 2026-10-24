/* 智慧校园 · 前端交互（无第三方依赖） */
(function () {
  "use strict";

  var SC = window.SC || {};
  var root = document.documentElement;
  var THEME_KEY = "sc-theme";

  // ---------------------------------------------------------------------
  // 基础工具
  // ---------------------------------------------------------------------

  function formatString(template, args) {
    var out = "";
    var idx = 0;
    for (var i = 0; i < template.length; i++) {
      var c = template.charAt(i);
      var n = template.charAt(i + 1);
      if (c === "{" && n === "{") {
        out += "{";
        i++;
      } else if (c === "}" && n === "}") {
        out += "}";
        i++;
      } else if (c === "{" && n === "}") {
        if (idx < args.length) out += String(args[idx++]);
        else out += "{}";
        i++;
      } else {
        out += c;
      }
    }
    return out;
  }

  function S(key) {
    var source = window.STR || {};
    var template = Object.prototype.hasOwnProperty.call(source, key) ? source[key] : key;
    return formatString(template, Array.prototype.slice.call(arguments, 1));
  }

  function $(sel, ctx) {
    return (ctx || document).querySelector(sel);
  }

  function $$(sel, ctx) {
    return Array.prototype.slice.call((ctx || document).querySelectorAll(sel));
  }

  var ICONS = {
    ok: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>',
    err: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 8v5M12 16h.01"/></svg>'
  };

  function toast(message, type) {
    var wrap = $(".toast-wrap");
    if (!wrap) {
      wrap = document.createElement("div");
      wrap.className = "toast-wrap";
      document.body.appendChild(wrap);
    }
    var el = document.createElement("div");
    el.className = "toast " + (type === "error" ? "err" : "ok");
    el.innerHTML = (type === "error" ? ICONS.err : ICONS.ok) + "<span></span>";
    el.querySelector("span").textContent = message;
    wrap.appendChild(el);
    setTimeout(function () {
      el.classList.add("leaving");
      setTimeout(function () {
        if (el.parentNode) el.parentNode.removeChild(el);
      }, 260);
    }, 3800);
  }

  async function api(path, body) {
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
      // API 返回英文技术信息；界面统一显示本地化提示。
      if (data.msg || data.message) console.error(data.msg || data.message);
      var msg;
      if (res.status === 401) msg = S("flash_login_expired");
      else if (res.status === 403) msg = S("flash_forbidden");
      else if (res.status >= 500) msg = S("flash_server_error");
      else msg = S("flash_request_failed");
      throw new Error(msg);
    }
    return data;
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

  function copyText(text) {
    if (!text) return Promise.resolve(false);
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text).then(
        function () {
          toast(S("toast_copy_ok"), "ok");
          return true;
        },
        function () {
          return fallbackCopy(text);
        }
      );
    }
    return fallbackCopy(text);
  }

  function fallbackCopy(text) {
    var ta = document.createElement("textarea");
    ta.value = text;
    ta.className = "copy-helper";
    document.body.appendChild(ta);
    ta.select();
    var ok = false;
    try {
      ok = document.execCommand("copy");
    } catch (e) {
      ok = false;
    }
    document.body.removeChild(ta);
    if (ok) toast(S("toast_copy_ok"), "ok");
    else toast(S("toast_copy_failed"), "error");
    return Promise.resolve(ok);
  }

  function formatBytes(n) {
    if (!n) return "0 B";
    if (n >= 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + " MB";
    if (n >= 1024) return Math.round(n / 1024) + " KB";
    return n + " B";
  }

  function confirmAction(opts) {
    opts = opts || {};
    return new Promise(function (resolve) {
      var d = $("#confirm-dialog");
      if (!d || typeof d.showModal !== "function") {
        resolve(window.confirm(opts.message || opts.title || S("toast_confirm_default")));
        return;
      }
      d.querySelector("[data-dialog-title]").textContent = opts.title || S("common_confirm_title");
      d.querySelector("[data-dialog-msg]").textContent = opts.message || "";
      var ok = d.querySelector("[data-dialog-confirm]");
      if (ok) {
        ok.textContent = opts.confirmText || S("common_confirm");
        ok.className = "btn " + (opts.danger === false ? "primary" : "danger");
      }
      function onClose() {
        d.removeEventListener("close", onClose);
        resolve(d.returnValue === "ok");
      }
      d.addEventListener("close", onClose);
      d.returnValue = "cancel";
      d.showModal();
    });
  }

  // ---------------------------------------------------------------------
  // 主题
  // ---------------------------------------------------------------------

  function applyTheme(theme) {
    root.dataset.theme = theme === "dark" ? "dark" : "light";
    try {
      localStorage.setItem(THEME_KEY, root.dataset.theme);
    } catch (e) {
      /* ignore */
    }
    var btn = $("#theme-toggle");
    if (btn) {
      btn.setAttribute(
        "aria-label",
        root.dataset.theme === "dark" ? S("common_theme_light") : S("common_theme_dark")
      );
    }
  }

  var themeBtn = $("#theme-toggle");
  if (themeBtn) {
    themeBtn.addEventListener("click", function () {
      applyTheme(root.dataset.theme === "dark" ? "light" : "dark");
    });
  }

  // ---------------------------------------------------------------------
  // Flash / toast / lightbox
  // ---------------------------------------------------------------------

  $$("[data-flash-close]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var flash = btn.closest(".flash");
      if (flash) flash.remove();
    });
  });

  $$(".flash").forEach(function (el) {
    setTimeout(function () {
      el.classList.add("is-hiding");
      setTimeout(function () {
        el.remove();
      }, 320);
    }, 6000);
  });

  document.addEventListener("click", function (e) {
    var copyBtn = e.target.closest("[data-copy]");
    if (copyBtn) {
      e.preventDefault();
      var value = copyBtn.getAttribute("data-copy");
      if (!value) {
        var key = copyBtn.closest(".auth-row, .member-key-row, .demo-keys");
        var keyEl = key && key.querySelector(".key, code");
        value = keyEl && (keyEl.getAttribute("data-full") || keyEl.textContent);
      }
      copyText(value);
      return;
    }

    var revealBtn = e.target.closest("[data-reveal]");
    if (revealBtn) {
      e.preventDefault();
      var row = revealBtn.closest(".auth-row");
      var keyEl = row && row.querySelector(".key");
      if (keyEl) {
        var full = keyEl.getAttribute("data-full") || "";
        var masked = keyEl.getAttribute("data-mask") || "······";
        var on = keyEl.dataset.revealed === "1";
        keyEl.textContent = on ? masked : full;
        keyEl.dataset.revealed = on ? "0" : "1";
        revealBtn.textContent = on ? S("common_show") : S("common_hide");
      }
      return;
    }

    var lightboxTrigger = e.target.closest("[data-lightbox]");
    if (lightboxTrigger) {
      e.preventDefault();
      var img = lightboxTrigger.querySelector("img");
      var dlg = $("#lightbox");
      if (img && dlg && typeof dlg.showModal === "function") {
        dlg.querySelector("img").src = img.src;
        dlg.querySelector("[data-lightbox-name]").textContent =
          lightboxTrigger.getAttribute("data-name") || S("common_attachment_preview");
        dlg.showModal();
      }
      return;
    }

    var delBtn = e.target.closest("[data-del]");
    if (delBtn) {
      e.preventDefault();
      e.stopPropagation();
      var id = delBtn.getAttribute("data-del");
      var name = delBtn.getAttribute("data-name") || S("common_object");
      confirmAction({
        title: S("toast_delete_confirm_title"),
        message: S("toast_delete_confirm_message", name),
        confirmText: S("common_delete"),
        danger: true
      }).then(function (ok) {
        if (!ok) return;
        setLoading(delBtn, true);
        api("/api/del-object", { auth: SC.auth, id: id })
          .then(function () {
            toast(S("toast_deleted"), "ok");
            var redirect = delBtn.getAttribute("data-redirect");
            if (redirect) {
              setTimeout(function () {
                location.href = redirect;
              }, 350);
            } else {
              setTimeout(function () {
                location.reload();
              }, 400);
            }
          })
          .catch(function (err) {
            setLoading(delBtn, false);
            toast(err.message, "error");
          });
      });
      return;
    }
  });

  document.addEventListener("keydown", function (e) {
    if (e.key !== "Enter" && e.key !== " ") return;
    var trigger = e.target.closest("[data-lightbox]");
    if (!trigger) return;
    e.preventDefault();
    trigger.click();
  });

  var lightbox = $("#lightbox");
  if (lightbox) {
    lightbox.addEventListener("click", function (e) {
      if (e.target === lightbox) lightbox.close();
    });
  }

  // ---------------------------------------------------------------------
  // 表单确认（用于 /user 下常规 POST 表单）
  // ---------------------------------------------------------------------

  document.addEventListener("submit", function (e) {
    var form = e.target;
    if (!form.matches("form[data-confirm]")) return;
    if (form.dataset.confirmed === "1") return;
    e.preventDefault();
    confirmAction({
      title: S("common_confirm_title"),
      message: form.getAttribute("data-confirm"),
      confirmText: form.getAttribute("data-confirm-text") || S("common_confirm"),
      danger: form.getAttribute("data-confirm-danger") !== "0"
    }).then(function (ok) {
      if (ok) {
        form.dataset.confirmed = "1";
        form.submit();
      }
    });
  });

  // ---------------------------------------------------------------------
  // /user 身份改名
  // ---------------------------------------------------------------------

  var renameDialog = $("#rename-dialog");
  var renameInput = $("#rename-input");
  var renameForm = $("#rename-form");
  var renameKey = "";

  if (renameDialog) {
    document.addEventListener("click", function (e) {
      var btn = e.target.closest("[data-rename]");
      if (!btn) return;
      e.preventDefault();
      e.stopPropagation();
      renameKey = btn.getAttribute("data-rename") || "";
      if (!renameKey || !renameInput) return;
      renameInput.value = btn.getAttribute("data-name") || "";
      renameDialog.showModal();
      setTimeout(function () {
        renameInput.focus();
        renameInput.select();
      }, 30);
    });

    renameDialog.addEventListener("click", function (e) {
      if (e.target === renameDialog) renameDialog.close();
    });

    renameDialog.addEventListener("close", function () {
      renameKey = "";
    });

    var renameCancel = $("#rename-cancel");
    if (renameCancel) {
      renameCancel.addEventListener("click", function () {
        renameDialog.close();
      });
    }
  }

  if (renameForm) {
    renameForm.addEventListener("submit", async function (e) {
      e.preventDefault();
      var save = $("#rename-save");
      var name = (renameInput.value || "").trim();
      if (!name) {
        toast(S("toast_enter_rename"), "error");
        renameInput.focus();
        return;
      }
      if (!renameKey) {
        toast(S("toast_rename_missing"), "error");
        return;
      }
      setLoading(save, true, S("toast_saving"));
      try {
        await api("/api/rename-auth", { auth: renameKey, name: name });
        toast(S("toast_renamed"), "ok");
        setTimeout(function () {
          location.href = "/user?ok=" + encodeURIComponent(S("toast_renamed"));
        }, 350);
      } catch (err) {
        setLoading(save, false);
        toast(err.message, "error");
      }
    });
  }

  // ---------------------------------------------------------------------
  // 登录页：演示密钥快速填充
  // ---------------------------------------------------------------------

  $$("[data-demo-key]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var input = $("#key-input");
      if (input) {
        input.value = btn.getAttribute("data-demo-key");
        input.focus();
        toast(S("toast_demo_filled"), "ok");
      }
    });
  });

  // ---------------------------------------------------------------------
  // 新建征集
  // ---------------------------------------------------------------------

  var submissionForm = $("#submission-form");
  if (submissionForm) {
    var noDeadline = $("#no-deadline");
    var deadlineInput = $("#sub-deadline");
    function syncDeadline() {
      if (!deadlineInput || !noDeadline) return;
      deadlineInput.disabled = noDeadline.checked;
    }
    if (noDeadline) {
      noDeadline.addEventListener("change", syncDeadline);
      syncDeadline();
    }

    submissionForm.addEventListener("submit", async function (e) {
      e.preventDefault();
      var btn = $("#submission-submit");
      var name = $("#sub-name").value.trim();
      var desc = $("#sub-desc").value.trim();
      var kind = $("#sub-type").value;
      var deadline = noDeadline && noDeadline.checked ? null : deadlineInput.value;
      if (!name) {
        toast(S("toast_submission_name_required"), "error");
        $("#sub-name").focus();
        return;
      }
      setLoading(btn, true, S("toast_publishing"));
      try {
        var r = await api("/api/new-submission", {
          auth: SC.auth,
          name: name,
          desc: desc,
          type: kind,
          deadLine: deadline ? deadline : null
        });
        toast(S("toast_submission_published"), "ok");
        setTimeout(function () {
          location.href =
            SC.home + "submissions/?ok=" + encodeURIComponent(S("toast_submission_published"));
        }, 300);
        void r;
      } catch (err) {
        setLoading(btn, false);
        toast(err.message, "error");
      }
    });
  }

  // ---------------------------------------------------------------------
  // 上传表单
  // ---------------------------------------------------------------------

  var uploadForm = $("#upload-form");
  if (uploadForm) {
    var fileInput = $("#file-input");
    var fileList = $("#file-list");
    var dropzone = $("#dropzone");
    var picked = [];
    var MAX_FILES = 8;
    var MAX_TOTAL = 18 * 1024 * 1024;

    function totalBytes() {
      return picked.reduce(function (sum, f) {
        return sum + (f.size || Math.floor((f.data.length * 3) / 4));
      }, 0);
    }

    function iconSvg(kind) {
      if (kind === "image") {
        return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8.5" cy="9" r="1.5"/><path d="m21 15-4.5-4.5L7 20"/></svg>';
      }
      return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6"/></svg>';
    }

    function renderFiles() {
      if (!fileList) return;
      fileList.innerHTML = "";
      picked.forEach(function (f, i) {
        var item = document.createElement("div");
        item.className = "file-item";
        var preview = document.createElement("div");
        preview.className = "file-preview";
        if (f.preview) {
          var img = document.createElement("img");
          img.src = f.preview;
          img.alt = "";
          preview.appendChild(img);
        } else {
          preview.innerHTML = iconSvg("file");
        }
        var meta = document.createElement("div");
        meta.className = "file-meta";
        var strong = document.createElement("strong");
        strong.textContent = f.name;
        var span = document.createElement("span");
        span.textContent = formatBytes(f.size) + " · " + f.mime;
        meta.appendChild(strong);
        meta.appendChild(span);
        var rm = document.createElement("button");
        rm.type = "button";
        rm.className = "file-remove";
        rm.setAttribute("aria-label", S("file_remove_aria"));
        rm.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>';
        rm.addEventListener("click", function () {
          picked.splice(i, 1);
          renderFiles();
        });
        item.appendChild(preview);
        item.appendChild(meta);
        item.appendChild(rm);
        fileList.appendChild(item);
      });
    }

    function readFile(file) {
      return new Promise(function (resolve, reject) {
        var reader = new FileReader();
        reader.onload = function () {
          var result = String(reader.result || "");
          var comma = result.indexOf(",");
          var base64 = comma >= 0 ? result.slice(comma + 1) : "";
          var mime = file.type || "application/octet-stream";
          resolve({
            name: file.name,
            mime: mime,
            data: base64,
            size: file.size,
            preview: mime.indexOf("image/") === 0 ? result : ""
          });
        };
        reader.onerror = function () {
          reject(new Error(S("file_read_failed", file.name)));
        };
        reader.readAsDataURL(file);
      });
    }

    async function addFiles(files) {
      var list = Array.prototype.slice.call(files || []);
      if (!list.length) return;
      if (picked.length >= MAX_FILES) {
        toast(S("toast_upload_limit", MAX_FILES), "error");
        return;
      }
      for (var i = 0; i < list.length; i++) {
        if (picked.length >= MAX_FILES) {
          toast(S("toast_upload_limit_reached", MAX_FILES), "error");
          break;
        }
        var file = list[i];
        if (file.size + totalBytes() > MAX_TOTAL) {
          toast(S("toast_upload_too_large"), "error");
          continue;
        }
        try {
          picked.push(await readFile(file));
          renderFiles();
        } catch (err) {
          toast(err.message, "error");
        }
      }
    }

    if (fileInput) {
      fileInput.addEventListener("change", function () {
        addFiles(fileInput.files).then(function () {
          fileInput.value = "";
        });
      });
    }

    if (dropzone) {
      ["dragenter", "dragover"].forEach(function (ev) {
        dropzone.addEventListener(ev, function (e) {
          e.preventDefault();
          dropzone.classList.add("drag");
        });
      });
      ["dragleave", "drop"].forEach(function (ev) {
        dropzone.addEventListener(ev, function (e) {
          e.preventDefault();
          dropzone.classList.remove("drag");
        });
      });
      dropzone.addEventListener("drop", function (e) {
        if (e.dataTransfer && e.dataTransfer.files) addFiles(e.dataTransfer.files);
      });
    }

    uploadForm.addEventListener("submit", async function (e) {
      e.preventDefault();
      var btn = $("#upload-submit");
      var title = $("#upload-title").value.trim();
      var desc = $("#upload-desc").value.trim();
      var submission = uploadForm.getAttribute("data-submission");
      if (!title) {
        toast(S("toast_upload_title_required"), "error");
        $("#upload-title").focus();
        return;
      }
      if (!submission) {
        toast(S("toast_missing_submission"), "error");
        return;
      }
      setLoading(btn, true, S("toast_submitting"));
      try {
        var r = await api("/api/upload", {
          auth: SC.auth,
          which: submission,
          title: title,
          desc: desc,
          data: picked.map(function (f) {
            return { name: f.name, mime: f.mime, data: f.data };
          })
        });
        toast(S("toast_uploaded"), "ok");
        setTimeout(function () {
          location.href =
            SC.home + "upload/" + r.id + "/?ok=" + encodeURIComponent(S("toast_uploaded"));
        }, 300);
      } catch (err) {
        setLoading(btn, false);
        toast(err.message, "error");
      }
    });
  }

  // ---------------------------------------------------------------------
  // 问卷：简介里的控件 -> JSON -> bot 提交（title=bot-upload）
  // ---------------------------------------------------------------------

  function surveyValues(form) {
    var groups = {};
    $$("[data-survey-kind]", form).forEach(function (el) {
      var key = el.getAttribute("data-survey-id") || el.id;
      if (!key) return;
      (groups[key] = groups[key] || []).push(el);
    });
    var out = {};
    Object.keys(groups).forEach(function (key) {
      var list = groups[key];
      var kind = list[0].getAttribute("data-survey-kind");
      if (kind === "switch") {
        out[key] = list[0].checked ? "on" : "off";
      } else if (kind === "checkbox") {
        out[key] = list
          .filter(function (el) {
            return el.checked;
          })
          .map(function (el) {
            return el.value || "on";
          })
          .join(",");
      } else {
        out[key] = (list[0].value || "").trim();
      }
    });
    return out;
  }

  $$("form[data-survey-form]").forEach(function (form) {
    form.addEventListener("submit", async function (e) {
      e.preventDefault();
      var btn = form.querySelector("[data-survey-submit]");
      var submission = form.getAttribute("data-submission");
      var values = surveyValues(form);
      // 多选组无法用原生 required 约束，这里补齐必填校验。
      var missing = null;
      $$("[data-survey-required]", form).forEach(function (el) {
        if (missing) return;
        var key = el.getAttribute("data-survey-id");
        if (!values[key]) missing = el;
      });
      if (missing) {
        toast(S("toast_survey_required"), "error");
        missing.focus();
        return;
      }
      if (!submission) {
        toast(S("toast_missing_submission"), "error");
        return;
      }
      setLoading(btn, true, S("toast_submitting"));
      try {
        var r = await api("/api/upload", {
          auth: SC.auth,
          which: submission,
          title: "bot-upload",
          desc: JSON.stringify(values),
          data: []
        });
        toast(S("toast_survey_submitted"), "ok");
        setTimeout(function () {
          location.href =
            SC.home + "upload/" + r.id + "/?ok=" + encodeURIComponent(S("toast_survey_submitted"));
        }, 300);
      } catch (err) {
        setLoading(btn, false);
        toast(err.message, "error");
      }
    });
  });

  // ---------------------------------------------------------------------
  // 回复
  // ---------------------------------------------------------------------

  $$("[data-quick-reply]").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var form = btn.closest(".panel-body, form") || document;
      var ta = form.querySelector("textarea");
      if (ta) {
        ta.value = btn.getAttribute("data-quick-reply");
        ta.focus();
      }
    });
  });

  $$("form[data-reply-form]").forEach(function (form) {
    form.addEventListener("submit", async function (e) {
      e.preventDefault();
      var btn = form.querySelector("button[type=submit]");
      var ta = form.querySelector("textarea");
      var content = (ta.value || "").trim();
      var upload = form.getAttribute("data-upload");
      if (!content) {
        toast(S("toast_reply_empty"), "error");
        ta.focus();
        return;
      }
      setLoading(btn, true, S("toast_sending"));
      try {
        await api("/api/reply", { auth: SC.auth, uploadID: upload, content: content });
        toast(S("toast_reply_published"), "ok");
        setTimeout(function () {
          var url = new URL(location.href);
          url.searchParams.set("ok", S("toast_reply_published"));
          location.href = url.toString();
        }, 300);
      } catch (err) {
        setLoading(btn, false);
        toast(err.message, "error");
      }
    });
  });

  // ---------------------------------------------------------------------
  // 导出征集数据（teacher/admin）
  // ---------------------------------------------------------------------

  var exportBtn = $("#export-btn");
  if (exportBtn) {
    exportBtn.addEventListener("click", async function () {
      var submission = exportBtn.getAttribute("data-submission");
      var name = exportBtn.getAttribute("data-name") || "submission";
      setLoading(exportBtn, true, S("toast_exporting"));
      try {
        var data = await api("/api/download", {
          auth: SC.auth,
          which: submission,
          match_mode: "All"
        });
        var blob = new Blob([JSON.stringify(data.datas, null, 2)], {
          type: "application/json;charset=utf-8"
        });
        var a = document.createElement("a");
        a.href = URL.createObjectURL(blob);
        a.download = name + "-uploads.json";
        document.body.appendChild(a);
        a.click();
        setTimeout(function () {
          URL.revokeObjectURL(a.href);
          a.remove();
        }, 500);
        toast(S("toast_exported", data.datas ? data.datas.length : 0), "ok");
      } catch (err) {
        toast(err.message, "error");
      } finally {
        setLoading(exportBtn, false);
      }
    });
  }

  // ---------------------------------------------------------------------
  // 账号中心：偏好设置 / 生成成员密钥
  // ---------------------------------------------------------------------

  var prefTheme = $("#pref-theme");
  var prefForm = $("#prefs-form");
  if (prefForm && prefTheme) {
    prefForm.addEventListener("submit", function () {
      try {
        var v = prefTheme.value;
        if (v === "system") localStorage.removeItem(THEME_KEY);
        else localStorage.setItem(THEME_KEY, v);
      } catch (e) {
        /* ignore */
      }
    });
  }

  var memberForm = $("#member-form");
  if (memberForm) {
    memberForm.addEventListener("submit", async function (e) {
      e.preventDefault();
      var btn = memberForm.querySelector("button[type=submit]");
      var adminSelect = $("#member-admin");
      var option = adminSelect && adminSelect.options[adminSelect.selectedIndex];
      var kind = $("#member-kind").value;
      var quantity = parseInt($("#member-quantity").value, 10);
      var result = $("#member-result");
      if (!option) {
        toast(S("toast_select_admin"), "error");
        return;
      }
      if (!quantity || quantity < 1 || quantity > 100) {
        toast(S("toast_quantity_range"), "error");
        return;
      }
      setLoading(btn, true, S("toast_generating"));
      try {
        var data = await api("/api/new-user", {
          auth: option.value,
          classID: option.getAttribute("data-class"),
          quantity: quantity,
          type: kind
        });
        if (result) {
          result.hidden = false;
          var title = document.createElement("h4");
          title.textContent = S("account_member_generated_title", data.keys.length);
          var list = document.createElement("div");
          list.className = "member-keys";
          (data.members || []).forEach(function (m) {
            var row = document.createElement("div");
            row.className = "member-key-row";
            var tag = document.createElement("span");
            tag.textContent = m.displayName + " · " + m.role;
            var code = document.createElement("code");
            code.textContent = m.key;
            var copy = document.createElement("button");
            copy.type = "button";
            copy.className = "btn ghost sm";
            copy.textContent = S("file_copy");
            copy.setAttribute("data-copy", m.key);
            row.appendChild(tag);
            row.appendChild(code);
            row.appendChild(copy);
            list.appendChild(row);
          });
          var all = document.createElement("button");
          all.type = "button";
          all.className = "btn outline sm mt-sm";
          all.textContent = S("file_copy_all");
          all.addEventListener("click", function () {
            copyText(data.keys.join("\n"));
          });
          result.innerHTML = "";
          result.appendChild(title);
          result.appendChild(list);
          result.appendChild(all);
        }
        toast(S("account_member_generated_toast", data.keys.length), "ok");
      } catch (err) {
        toast(err.message, "error");
      } finally {
        setLoading(btn, false);
      }
    });
  }

  // ---------------------------------------------------------------------
  // 表单里的字数提示
  // ---------------------------------------------------------------------

  $$("[data-counter]").forEach(function (el) {
    var target = document.getElementById(el.getAttribute("data-counter"));
    if (!target) return;
    var max = parseInt(el.getAttribute("data-max") || "0", 10);
    function update() {
      var len = (target.value || "").length;
      el.textContent = max ? len + " / " + max : String(len);
      el.classList.toggle("over", max > 0 && len > max);
    }
    target.addEventListener("input", update);
    update();
  });

  // ---------------------------------------------------------------------
  // 轻提示：URL 中的 ok / err 已经由服务端渲染为 flash
  // ---------------------------------------------------------------------

  if (SC.flashError) toast(SC.flashError, "error");
})();
