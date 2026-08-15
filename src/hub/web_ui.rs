//! The hub's management page: one inline HTML document, no build pipeline.
//!
//! The page drives the JSON API under `/api` with the session cookie and
//! CSRF token from `/api/session`. Keeping it a single const string keeps
//! deployment to "run the binary".

pub const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Chatstronomy hub</title>
<style>
  :root {
    --bg: #0d1117; --panel: #161b22; --border: #30363d; --text: #e6edf3;
    --muted: #8b949e; --accent: #58a6ff; --good: #3fb950; --bad: #f85149;
    --warn: #d29922;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; background: var(--bg); color: var(--text);
    font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif;
  }
  .wrap { max-width: 880px; margin: 0 auto; padding: 1.5rem 1rem 4rem; }
  header { display: flex; align-items: center; gap: .75rem; margin-bottom: 1.5rem; }
  header h1 { font-size: 1.3rem; margin: 0; flex: 1; }
  header h1 span { color: var(--muted); font-weight: normal; }
  .avatar { width: 28px; height: 28px; border-radius: 50%; }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .card {
    background: var(--panel); border: 1px solid var(--border);
    border-radius: 8px; padding: 1rem; margin-bottom: 1rem;
  }
  .card h2 { margin: 0 0 .5rem; font-size: 1.05rem; }
  .row { display: flex; align-items: center; gap: .6rem; flex-wrap: wrap; }
  .grow { flex: 1; }
  .badge {
    font-size: .75rem; padding: .1rem .5rem; border-radius: 999px;
    border: 1px solid var(--border); color: var(--muted); white-space: nowrap;
  }
  .badge.good { color: var(--good); border-color: var(--good); }
  .badge.bad { color: var(--bad); border-color: var(--bad); }
  .badge.warn { color: var(--warn); border-color: var(--warn); }
  button {
    background: #21262d; color: var(--text); border: 1px solid var(--border);
    border-radius: 6px; padding: .35rem .8rem; cursor: pointer; font: inherit;
    font-size: .85rem;
  }
  button:hover { border-color: var(--accent); }
  button.primary { background: #1f6feb; border-color: #1f6feb; }
  button.danger { color: var(--bad); }
  input, select {
    background: var(--bg); color: var(--text); border: 1px solid var(--border);
    border-radius: 6px; padding: .35rem .5rem; font: inherit; font-size: .85rem;
  }
  input.short { width: 10rem; }
  table { width: 100%; border-collapse: collapse; margin-top: .5rem; }
  th, td { text-align: left; padding: .4rem .5rem; border-top: 1px solid var(--border); }
  th { color: var(--muted); font-weight: normal; font-size: .8rem; }
  .telescope-row td { vertical-align: middle; }
  .token-box {
    background: var(--bg); border: 1px dashed var(--warn); border-radius: 6px;
    padding: .75rem; margin-top: .75rem; word-break: break-all;
    font-family: ui-monospace, monospace; font-size: .85rem;
  }
  .hint { color: var(--muted); font-size: .8rem; }
  .error { color: var(--bad); margin: .5rem 0; }
  #toast {
    position: fixed; bottom: 1rem; left: 50%; transform: translateX(-50%);
    background: var(--panel); border: 1px solid var(--border);
    border-radius: 8px; padding: .5rem 1rem; display: none;
  }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <h1>Chatstronomy hub <span>· space | cat</span></h1>
    <span id="whoami"></span>
  </header>
  <div id="app"><p class="hint">Loading…</p></div>
</div>
<div id="toast"></div>
<script>
"use strict";
let CSRF = null;

const app = document.getElementById("app");
const esc = (s) => String(s ?? "").replace(/[&<>"']/g,
  (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));

function toast(msg) {
  const el = document.getElementById("toast");
  el.textContent = msg;
  el.style.display = "block";
  setTimeout(() => { el.style.display = "none"; }, 4000);
}

async function api(path, opts = {}) {
  opts.headers = Object.assign({}, opts.headers);
  if (opts.method && opts.method !== "GET") {
    opts.headers["x-csrf-token"] = CSRF;
    if (opts.body) opts.headers["content-type"] = "application/json";
  }
  const res = await fetch(path, opts);
  if (!res.ok) {
    throw new Error(await res.text() || res.statusText);
  }
  const type = res.headers.get("content-type") || "";
  return type.includes("json") ? res.json() : res.text();
}

async function boot() {
  const session = await api("/api/session");
  const who = document.getElementById("whoami");
  if (!session.authenticated) {
    who.innerHTML = "";
    app.innerHTML =
      '<div class="card"><h2>Sign in</h2>' +
      '<p>Manage your server’s telescopes: pair rigs, route channels, and control who can send commands.</p>' +
      '<p><a href="/login"><button class="primary">Log in with Discord</button></a></p></div>';
    return;
  }
  CSRF = session.csrf_token;
  const u = session.user;
  who.innerHTML = (u.avatar_url ? '<img class="avatar" src="' + esc(u.avatar_url) + '" alt=""> ' : "") +
    esc(u.username) + ' · <a href="/logout">log out</a>';
  await renderGuilds();
}

async function renderGuilds() {
  app.innerHTML = '<p class="hint">Loading your servers…</p>';
  let data;
  try { data = await api("/api/guilds"); }
  catch (e) { app.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; return; }
  if (!data.guilds.length) {
    app.innerHTML = '<div class="card"><p>No servers where you hold <b>Manage Server</b>. ' +
      "Ask a server admin, or check your Discord permissions.</p></div>";
    return;
  }
  app.innerHTML = "";
  for (const g of data.guilds) {
    const card = document.createElement("div");
    card.className = "card";
    const botBadge = g.bot_installed === null ? "" :
      g.bot_installed ? '<span class="badge good">bot installed</span>'
                      : '<span class="badge bad">bot not installed</span>';
    const regBadge = g.registered ? '<span class="badge good">registered</span>'
                                  : '<span class="badge">not registered</span>';
    let actions = "";
    if (g.bot_installed === false && g.install_url) {
      actions += '<a href="' + esc(g.install_url) + '" target="_blank" rel="noopener">' +
        '<button class="primary">Add bot to server</button></a> ';
    }
    if (!g.registered) {
      actions += '<button data-register="' + esc(g.id) + '">Register</button>';
    }
    card.innerHTML =
      '<div class="row"><h2 class="grow">' + esc(g.name) + "</h2>" +
      regBadge + " " + botBadge + " " + actions + "</div>" +
      '<div class="telescopes" data-guild="' + esc(g.id) + '"></div>';
    app.appendChild(card);
    if (g.registered) renderTelescopes(g.id, card.querySelector(".telescopes"));
  }
  app.querySelectorAll("[data-register]").forEach((btn) => {
    btn.onclick = async () => {
      try {
        await api("/api/guilds/" + btn.dataset.register + "/register", { method: "POST" });
        toast("Server registered");
        renderGuilds();
      } catch (e) { toast(e.message); }
    };
  });
}

async function renderTelescopes(guildId, el) {
  let data;
  try { data = await api("/api/guilds/" + guildId + "/telescopes"); }
  catch (e) { el.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; return; }
  let html =
    '<table><tr><th>Telescope</th><th>Channel ID</th><th>Cooldown (s)</th>' +
    "<th>Write commands</th><th>Allowed role IDs</th><th></th></tr>";
  for (const t of data.telescopes) {
    const connected = t.connected ? '<span class="badge good">connected</span>'
                                  : '<span class="badge warn">offline</span>';
    html +=
      '<tr class="telescope-row" data-id="' + t.id + '">' +
      "<td><b>" + esc(t.name) + "</b><br>" + connected + "</td>" +
      '<td><input class="short f-channel" value="' + esc(t.discord_channel_id ?? "") +
        '" placeholder="channel id"></td>' +
      '<td><input class="short f-cooldown" value="' + t.image_cooldown_seconds + '" size="5"></td>' +
      '<td><select class="f-policy">' +
        '<option value="disabled"' + (t.write_policy === "disabled" ? " selected" : "") + ">disabled</option>" +
        '<option value="roles"' + (t.write_policy === "roles" ? " selected" : "") + ">roles</option>" +
      "</select></td>" +
      '<td><input class="short f-roles" value="' + esc(t.allowed_role_ids.join(", ")) +
        '" placeholder="role ids"></td>' +
      "<td><button class=\"b-save\">Save</button> " +
      "<button class=\"b-token\">Pairing token</button> " +
      "<button class=\"b-delete danger\">Delete</button></td></tr>";
  }
  html += "</table>" +
    '<div class="row" style="margin-top:.6rem">' +
    '<input class="new-name" placeholder="new telescope name">' +
    '<button class="b-create">Create telescope</button>' +
    '<span class="hint">Each telescope pairs one N.I.N.A. profile or relay agent.</span></div>' +
    '<div class="token-out"></div>';
  el.innerHTML = html;

  el.querySelector(".b-create").onclick = async () => {
    const name = el.querySelector(".new-name").value.trim();
    if (!name) return;
    try {
      await api("/api/guilds/" + guildId + "/telescopes",
        { method: "POST", body: JSON.stringify({ name }) });
      toast("Telescope created");
      renderTelescopes(guildId, el);
    } catch (e) { toast(e.message); }
  };

  el.querySelectorAll(".telescope-row").forEach((row) => {
    const id = row.dataset.id;
    row.querySelector(".b-save").onclick = async () => {
      const channelRaw = row.querySelector(".f-channel").value.trim();
      const roles = row.querySelector(".f-roles").value
        .split(",").map((s) => s.trim()).filter(Boolean);
      const body = {
        discord_channel_id: channelRaw === "" ? null : channelRaw,
        image_cooldown_seconds: parseInt(row.querySelector(".f-cooldown").value, 10) || 0,
        write_policy: row.querySelector(".f-policy").value,
        allowed_role_ids: roles,
      };
      try {
        await api("/api/telescopes/" + id, { method: "PATCH", body: JSON.stringify(body) });
        toast("Saved");
        renderTelescopes(guildId, el);
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-delete").onclick = async () => {
      if (!confirm("Delete this telescope? Its credentials and tokens go with it.")) return;
      try {
        await api("/api/telescopes/" + id, { method: "DELETE" });
        toast("Deleted");
        renderTelescopes(guildId, el);
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-token").onclick = async () => {
      try {
        const out = await api("/api/telescopes/" + id + "/pairing-token", { method: "POST" });
        const box = el.querySelector(".token-out");
        box.innerHTML =
          '<div class="token-box">Pairing token (shown once, valid ' +
          Math.round(out.expires_in_seconds / 60) + " minutes):<br><b>" + esc(out.token) +
          '</b><br><button class="b-copy">Copy</button> ' +
          '<span class="hint">Paste into the N.I.N.A. plugin or the relay config, ' +
          "then connect. Issuing a new token cancels this one.</span></div>";
        box.querySelector(".b-copy").onclick = () => {
          navigator.clipboard.writeText(out.token).then(() => toast("Copied"));
        };
      } catch (e) { toast(e.message); }
    };
  });
}

boot().catch((e) => { app.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; });
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_has_the_expected_hooks() {
        // The page drives these endpoints; renaming one must fail a test.
        for needle in [
            "/api/session",
            "/api/guilds",
            "/pairing-token",
            "/login",
            "/logout",
            "x-csrf-token",
        ] {
            assert!(INDEX_HTML.contains(needle), "missing {needle}");
        }
    }
}
