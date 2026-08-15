//! The hub's management page: one inline HTML document, no build pipeline.
//!
//! Structure mirrors the data model: "My telescopes" (user-owned rigs —
//! pairing, cooldown, attach/share) on top, then one card per server with
//! its attachments (per-server permissions and channel destinations).
//! Channels and roles come from Discord via `/api/guilds/{id}/options` —
//! nobody types an ID anywhere.

pub const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Chatstronomy hub</title>
<style>
  :root {
    --bg: #0b0e14; --panel: #141922; --panel2: #1a2130; --border: #2a3245;
    --text: #e6edf3; --muted: #8b96a8; --accent: #58a6ff; --accent2: #1f6feb;
    --good: #3fb950; --bad: #f85149; --warn: #d29922; --radius: 10px;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; background: var(--bg); color: var(--text);
    font: 15px/1.55 system-ui, -apple-system, "Segoe UI", sans-serif;
  }
  .wrap { max-width: 860px; margin: 0 auto; padding: 1.5rem 1rem 4rem; }
  header { display: flex; align-items: center; gap: .75rem; margin-bottom: 1.25rem; }
  header h1 { font-size: 1.25rem; margin: 0; flex: 1; letter-spacing: .01em; }
  header h1 span { color: var(--muted); font-weight: normal; font-size: .9rem; }
  .avatar { width: 28px; height: 28px; border-radius: 50%; vertical-align: middle; }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }

  .card {
    background: var(--panel); border: 1px solid var(--border);
    border-radius: var(--radius); padding: 1.1rem 1.2rem; margin-bottom: 1.1rem;
  }
  .card > .head { display: flex; align-items: center; gap: .6rem; flex-wrap: wrap; }
  .card h2 { margin: 0; font-size: 1.05rem; flex: 1; }
  .sub {
    background: var(--panel2); border: 1px solid var(--border);
    border-radius: var(--radius); padding: .9rem 1rem; margin-top: .9rem;
  }
  .sub .head { display: flex; align-items: center; gap: .6rem; margin-bottom: .6rem; flex-wrap: wrap; }
  .sub .head b { font-size: 1rem; }
  .grow { flex: 1; }

  .badge {
    font-size: .72rem; padding: .12rem .55rem; border-radius: 999px;
    border: 1px solid var(--border); color: var(--muted); white-space: nowrap;
  }
  .badge.good { color: var(--good); border-color: color-mix(in srgb, var(--good) 60%, transparent); }
  .badge.bad { color: var(--bad); border-color: color-mix(in srgb, var(--bad) 60%, transparent); }
  .badge.warn { color: var(--warn); border-color: color-mix(in srgb, var(--warn) 60%, transparent); }

  button {
    background: #222938; color: var(--text); border: 1px solid var(--border);
    border-radius: 7px; padding: .4rem .85rem; cursor: pointer; font: inherit;
    font-size: .85rem; transition: border-color .12s, background .12s;
  }
  button:hover { border-color: var(--accent); }
  button.primary { background: var(--accent2); border-color: var(--accent2); }
  button.primary:hover { filter: brightness(1.1); }
  button.subtle { background: transparent; color: var(--muted); }
  button.danger { color: var(--bad); }
  button.danger:hover { border-color: var(--bad); }
  .actions { display: flex; gap: .5rem; flex-wrap: wrap; margin-top: .8rem; }
  .actions .spacer { flex: 1; }

  .fields {
    display: grid; grid-template-columns: repeat(auto-fit, minmax(215px, 1fr));
    gap: .8rem; margin-top: .4rem;
  }
  .field label {
    display: block; font-size: .75rem; color: var(--muted);
    margin-bottom: .25rem; text-transform: uppercase; letter-spacing: .05em;
  }
  input, select {
    width: 100%; background: var(--bg); color: var(--text);
    border: 1px solid var(--border); border-radius: 7px; padding: .45rem .55rem;
    font: inherit; font-size: .88rem;
  }
  .row select, .row input, .newrow select, .newrow input { width: auto; }
  .row { display: flex; gap: .5rem; align-items: center; flex-wrap: wrap; }
  .chips { display: flex; flex-wrap: wrap; gap: .4rem; }
  .chip {
    display: inline-flex; align-items: center; gap: .35rem;
    border: 1px solid var(--border); border-radius: 999px;
    padding: .2rem .7rem; cursor: pointer; font-size: .82rem; color: var(--muted);
    user-select: none;
  }
  .chip input { display: none; }
  .chip.on { color: var(--text); border-color: var(--accent); background: #1b2740; }
  .chip .rm-route {
    background: none; border: none; padding: 0 0 0 .15rem; color: var(--muted);
    cursor: pointer; font-size: .8rem;
  }
  .chip .rm-route:hover { color: var(--bad); }

  .newrow { display: flex; gap: .5rem; margin-top: .9rem; align-items: center; flex-wrap: wrap; }
  .newrow input { max-width: 240px; }
  .token-box {
    background: var(--bg); border: 1px dashed var(--warn); border-radius: 8px;
    padding: .8rem .9rem; margin-top: .8rem; word-break: break-all;
    font-family: ui-monospace, monospace; font-size: .85rem;
  }
  .hint { color: var(--muted); font-size: .8rem; }
  .error { color: var(--bad); margin: .5rem 0; }
  .banner {
    border: 1px solid var(--warn); background: color-mix(in srgb, var(--warn) 12%, transparent);
    border-radius: var(--radius); padding: .7rem 1rem; margin-bottom: 1rem;
    font-size: .88rem;
  }
  #toast {
    position: fixed; bottom: 1.1rem; left: 50%; transform: translateX(-50%);
    background: var(--panel2); border: 1px solid var(--border);
    border-radius: 8px; padding: .55rem 1.1rem; display: none; z-index: 10;
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
let GUILDS = [];

const app = document.getElementById("app");
const esc = (s) => String(s ?? "").replace(/[&<>"']/g,
  (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));

function toast(msg) {
  const el = document.getElementById("toast");
  el.textContent = msg;
  el.style.display = "block";
  clearTimeout(el._t);
  el._t = setTimeout(() => { el.style.display = "none"; }, 4000);
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
      '<div class="card"><h2>Bring your observatory into Discord</h2>' +
      '<p>Add your telescopes, attach them to your servers, and let everyone ' +
      "watch sessions unfold — images, autofocus runs, guiding graphs, and " +
      "slash commands to drive the mount.</p>" +
      '<p><a href="/login"><button class="primary">Log in with Discord</button></a></p></div>';
    return;
  }
  CSRF = session.csrf_token;
  const u = session.user;
  who.innerHTML = (u.avatar_url ? '<img class="avatar" src="' + esc(u.avatar_url) + '" alt=""> ' : "") +
    esc(u.username) + ' &nbsp;·&nbsp; <a href="/logout">log out</a>';
  await renderAll();
}

async function renderAll() {
  app.innerHTML = '<p class="hint">Loading…</p>';
  let guilds, mine;
  try {
    [guilds, mine] = await Promise.all([api("/api/guilds"), api("/api/telescopes")]);
  } catch (e) { app.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; return; }
  GUILDS = guilds.guilds;
  app.innerHTML = "";
  if (guilds.bot_configured === false) {
    const banner = document.createElement("div");
    banner.className = "banner";
    banner.innerHTML = "<b>This hub is running without a Discord bot token.</b> " +
      "Channel and role pickers, install checks, notifications, and slash commands " +
      "are all disabled until the hub operator sets <code>discord.bot_token</code> " +
      "and restarts the hub.";
    app.appendChild(banner);
  }
  renderMyTelescopes(mine.telescopes);
  for (const g of GUILDS) {
    renderGuildCard(g);
  }
  if (!GUILDS.length) {
    const note = document.createElement("div");
    note.className = "card";
    note.innerHTML = "<p>No servers where you hold <b>Manage Server</b>. " +
      "Ask a server admin, or check your Discord permissions.</p>";
    app.appendChild(note);
  }
}

// ---------- My telescopes ----------

function attachTargets(t) {
  // Registered, manageable servers this telescope is not attached to yet.
  const attached = new Set(t.attachments.map((a) => a.guild_id));
  return GUILDS.filter((g) => g.registered && !attached.has(g.id));
}

function renderMyTelescopes(telescopes) {
  const card = document.createElement("div");
  card.className = "card";
  let html = '<div class="head"><h2>My telescopes</h2>' +
    '<span class="hint">One per N.I.N.A. profile or relay agent — yours across every server.</span></div>';
  for (const t of telescopes) {
    const connected = t.connected
      ? '<span class="badge good">connected</span>'
      : '<span class="badge warn">waiting for rig</span>';
    const where = t.attachments.length
      ? t.attachments.map((a) =>
          esc(a.guild_name || a.guild_id) + (a.can_command ? "" : " (feed)")).join(", ")
      : "not attached to any server yet";
    const targets = attachTargets(t);
    const attachRow = targets.length
      ? '<select class="f-attach">' + targets.map((g) =>
          '<option value="' + esc(g.id) + '">' + esc(g.name) + "</option>").join("") +
        '</select><button class="b-attach">Attach to server</button>'
      : '<span class="hint">Attached to all your registered servers.</span>';
    html +=
      '<div class="sub telescope" data-id="' + t.id + '">' +
      '<div class="head"><b>' + esc(t.name) + "</b>" + connected +
      '<span class="hint">' + where + '</span><span class="grow"></span></div>' +
      '<div class="row">' +
      '<span class="hint">Image cooldown (s)</span>' +
      '<input class="f-cooldown" type="number" min="0" max="86400" style="max-width:6.5rem" value="' +
      t.image_cooldown_seconds + '">' +
      '<button class="b-save">Save</button>' +
      attachRow +
      "</div>" +
      '<div class="actions">' +
      '<button class="primary b-token">Pair a rig…</button>' +
      '<button class="b-share">Share code…</button>' +
      '<span class="spacer"></span>' +
      '<button class="subtle danger b-revoke">Revoke access</button>' +
      '<button class="subtle danger b-delete">Delete</button>' +
      "</div></div>";
  }
  html +=
    '<div class="newrow">' +
    '<input class="new-name" placeholder="telescope name (e.g. c925)">' +
    '<button class="b-create">Add telescope</button></div>' +
    '<div class="token-out"></div>';
  card.innerHTML = html;
  app.appendChild(card);

  card.querySelector(".b-create").onclick = async () => {
    const name = card.querySelector(".new-name").value.trim();
    if (!name) return;
    try {
      await api("/api/telescopes", { method: "POST", body: JSON.stringify({ name }) });
      toast("Telescope added — attach it to a server, then pair the rig");
      renderAll();
    } catch (e) { toast(e.message); }
  };

  card.querySelectorAll(".telescope").forEach((row) => {
    const id = row.dataset.id;
    row.querySelector(".b-save").onclick = async () => {
      const body = {
        image_cooldown_seconds: parseInt(row.querySelector(".f-cooldown").value, 10) || 0,
      };
      try {
        await api("/api/telescopes/" + id, { method: "PATCH", body: JSON.stringify(body) });
        toast("Saved");
      } catch (e) { toast(e.message); }
    };
    const attach = row.querySelector(".b-attach");
    if (attach) {
      attach.onclick = async () => {
        const guild = row.querySelector(".f-attach").value;
        try {
          await api("/api/telescopes/" + id + "/attach",
            { method: "POST", body: JSON.stringify({ guild_id: guild }) });
          toast("Attached — pick channels on the server card below");
          renderAll();
        } catch (e) { toast(e.message); }
      };
    }
    row.querySelector(".b-token").onclick = async () => {
      try {
        const out = await api("/api/telescopes/" + id + "/pairing-token", { method: "POST" });
        showToken(card, "Pairing token (shown once, valid " +
          Math.round(out.expires_in_seconds / 60) + " minutes)", out.token,
          "Paste into the N.I.N.A. plugin or the relay config, then connect. " +
          "Issuing a new token cancels this one.");
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-share").onclick = async () => {
      try {
        const out = await api("/api/telescopes/" + id + "/share-code", { method: "POST" });
        showToken(card, "Share code (valid " +
          Math.round(out.expires_in_seconds / 86400) + " days, single use)", out.code,
          "Give this to a manager of another server. They redeem it on their server " +
          "card against one of their channels. Their server gets the feed and read " +
          "commands; only servers you attach directly can drive the scope.");
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-revoke").onclick = async () => {
      if (!confirm("Revoke this telescope's rig credentials? The rig is disconnected " +
        "and must re-pair with a new token.")) return;
      try {
        await api("/api/telescopes/" + id + "/credentials", { method: "DELETE" });
        await api("/api/telescopes/" + id + "/pairing-tokens", { method: "DELETE" });
        toast("Access revoked");
        renderAll();
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-delete").onclick = async () => {
      if (!confirm("Delete this telescope everywhere? All attachments, destinations, " +
        "and credentials go with it.")) return;
      try {
        await api("/api/telescopes/" + id, { method: "DELETE" });
        toast("Deleted");
        renderAll();
      } catch (e) { toast(e.message); }
    };
  });
}

function showToken(card, title, value, hint) {
  const box = card.querySelector(".token-out");
  box.innerHTML = '<div class="token-box">' + esc(title) + ":<br><b>" + esc(value) +
    '</b><br><button class="b-copy">Copy</button> <span class="hint">' + hint + "</span></div>";
  box.querySelector(".b-copy").onclick = () => {
    navigator.clipboard.writeText(value).then(() => toast("Copied"));
  };
}

// ---------- Server cards ----------

function renderGuildCard(g) {
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
  if (!g.registered && g.bot_installed !== false) {
    actions += '<button class="primary b-register">Set up this server</button>';
  }
  card.innerHTML =
    '<div class="head"><h2>' + esc(g.name) + "</h2>" +
    regBadge + " " + botBadge + " " + actions + "</div>" +
    '<div class="attachments"></div>';
  app.appendChild(card);
  const registerBtn = card.querySelector(".b-register");
  if (registerBtn) {
    registerBtn.onclick = async () => {
      try {
        await api("/api/guilds/" + g.id + "/register", { method: "POST" });
        toast("Server registered");
        renderAll();
      } catch (e) { toast(e.message); }
    };
  }
  if (g.registered) renderAttachments(g, card.querySelector(".attachments"));
}

const POLICY_OPTIONS = [
  ["admins", "Server managers"],
  ["roles", "Managers + selected roles"],
  ["disabled", "Disabled"],
];

function addChannelSelect(options, used, cls) {
  const free = options.channels.filter((c) => !used.includes(c.id));
  if (!options.channels.length) {
    const why = options.bot_configured === false
      ? "hub has no bot token"
      : "none visible to the bot — check its channel permissions";
    return '<select class="' + cls + '" disabled><option value="">no channels (' +
      why + ")</option></select>";
  }
  if (!free.length) {
    return '<select class="' + cls + '" disabled><option value="">all channels routed</option></select>';
  }
  return '<select class="' + cls + '">' + free.map((c) =>
    '<option value="' + esc(c.id) + '">#' + esc(c.name) + "</option>").join("") + "</select>";
}

function roleChips(options, selected) {
  if (!options.roles.length) {
    return '<span class="hint">No assignable roles found in this server.</span>';
  }
  return '<div class="chips">' + options.roles.map((r) => {
    const on = selected.includes(r.id);
    return '<label class="chip' + (on ? " on" : "") + '"><input type="checkbox" value="' +
      esc(r.id) + '"' + (on ? " checked" : "") + ">@" + esc(r.name) + "</label>";
  }).join("") + "</div>";
}

function destinationChips(a) {
  if (!a.channels.length) {
    return '<span class="hint">No channels yet — this feed is not posting here.</span>';
  }
  return '<div class="chips">' + a.channels.map((route) => {
    const name = route.channel_name ? '#' + esc(route.channel_name)
                                    : "channel " + esc(route.channel_id);
    return '<span class="chip on">' + name +
      ' <button type="button" class="rm-route" data-route="' + route.route_id +
      '" title="Remove">✕</button></span>';
  }).join("") + "</div>";
}

async function renderAttachments(g, el) {
  let data, options;
  try {
    [data, options] = await Promise.all([
      api("/api/guilds/" + g.id + "/attachments"),
      api("/api/guilds/" + g.id + "/options"),
    ]);
  } catch (e) { el.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; return; }

  const usedChannels = data.attachments.flatMap((a) => a.channels.map((r) => r.channel_id));
  let html = "";
  for (const a of data.attachments) {
    const connected = a.connected
      ? '<span class="badge good">connected</span>'
      : '<span class="badge warn">waiting for rig</span>';
    const kind = a.can_command
      ? '<span class="badge good">commands enabled</span>'
      : '<span class="badge">feed only</span>';
    const owner = a.owned_by_me ? "" :
      '<span class="hint">from ' + esc(a.owner_name) + "</span>";
    const policySelect = '<select class="f-policy">' + POLICY_OPTIONS.map(([v, label]) =>
      '<option value="' + v + '"' + (a.write_policy === v ? " selected" : "") + ">" +
      label + "</option>").join("") + "</select>";
    html +=
      '<div class="sub attachment" data-id="' + a.attachment_id + '">' +
      '<div class="head"><b>' + esc(a.telescope_name) + "</b>" + owner + connected + kind +
      '<span class="grow"></span>' +
      '<button class="subtle danger b-detach">Detach</button></div>' +
      '<div class="field"><label>Posts to channels</label>' + destinationChips(a) +
      '<div class="row" style="margin-top:.5rem">' +
      addChannelSelect(options, usedChannels, "f-addchan") +
      '<button class="b-addchan">Add channel</button></div></div>' +
      (a.can_command
        ? '<div class="fields" style="margin-top:.8rem">' +
          '<div class="field"><label>Who can send commands here</label>' + policySelect + "</div>" +
          "</div>" +
          '<div class="field roles-field" style="margin-top:.7rem' +
          (a.write_policy === "roles" ? "" : ";display:none") + '">' +
          "<label>Roles allowed to send commands (managers always can)</label>" +
          roleChips(options, a.allowed_role_ids) + "</div>" +
          '<div class="actions"><button class="primary b-savepolicy">Save permissions</button></div>'
        : '<p class="hint" style="margin:.6rem 0 0">This server receives the feed and read ' +
          "commands; it cannot drive the telescope.</p>") +
      "</div>";
  }
  html +=
    '<div class="newrow">' +
    '<input class="share-code" placeholder="share code (cssh_…)">' +
    addChannelSelect(options, usedChannels, "sub-channel") +
    '<button class="b-subscribe">Add shared telescope</button>' +
    '<span class="hint">Redeem a code from another server’s telescope owner.</span></div>';
  el.innerHTML = html;

  el.querySelector(".b-subscribe").onclick = async () => {
    const code = el.querySelector(".share-code").value.trim();
    const channel = el.querySelector(".sub-channel").value;
    if (!code || !channel) { toast("Enter a share code and pick a channel"); return; }
    try {
      const out = await api("/api/guilds/" + g.id + "/subscribe",
        { method: "POST", body: JSON.stringify({ code, channel_id: channel }) });
      toast("Subscribed to " + out.telescope_name);
      renderAll();
    } catch (e) { toast(e.message); }
  };

  el.querySelectorAll(".attachment").forEach((row) => {
    const id = row.dataset.id;
    const rolesField = row.querySelector(".roles-field");
    const policy = row.querySelector(".f-policy");
    if (policy) {
      policy.onchange = (ev) => {
        if (rolesField) rolesField.style.display = ev.target.value === "roles" ? "" : "none";
      };
    }
    row.querySelectorAll(".chip input").forEach((box) => {
      box.onchange = () => box.closest(".chip").classList.toggle("on", box.checked);
    });
    const save = row.querySelector(".b-savepolicy");
    if (save) {
      save.onclick = async () => {
        const roles = [...row.querySelectorAll(".chip input:checked")].map((box) => box.value);
        try {
          await api("/api/attachments/" + id, {
            method: "PATCH",
            body: JSON.stringify({ write_policy: policy.value, allowed_role_ids: roles }),
          });
          toast("Permissions saved");
        } catch (e) { toast(e.message); }
      };
    }
    row.querySelector(".b-addchan").onclick = async () => {
      const box = row.querySelector(".f-addchan");
      if (box.disabled || !box.value) return;
      try {
        await api("/api/attachments/" + id + "/channels",
          { method: "POST", body: JSON.stringify({ channel_id: box.value }) });
        toast("Channel added");
        renderAll();
      } catch (e) { toast(e.message); }
    };
    row.querySelectorAll(".rm-route").forEach((btn) => {
      btn.onclick = async () => {
        try {
          await api("/api/attachments/" + id + "/channels/" + btn.dataset.route,
            { method: "DELETE" });
          toast("Channel removed");
          renderAll();
        } catch (e) { toast(e.message); }
      };
    });
    row.querySelector(".b-detach").onclick = async () => {
      if (!confirm("Detach this telescope from this server? Its channels here are removed.")) return;
      try {
        await api("/api/attachments/" + id, { method: "DELETE" });
        toast("Detached");
        renderAll();
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
            "/api/telescopes",
            "/attachments",
            "/options",
            "/pairing-token",
            "/share-code",
            "/subscribe",
            "/login",
            "/logout",
            "x-csrf-token",
        ] {
            assert!(INDEX_HTML.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn page_reflects_the_ownership_model() {
        for needle in [
            "My telescopes",
            "Attach to server",
            "feed only",
            "commands enabled",
            "Detach",
        ] {
            assert!(INDEX_HTML.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn page_uses_pickers_not_id_inputs() {
        // Channels and roles are picked from Discord data, never typed.
        assert!(INDEX_HTML.contains("f-addchan"));
        assert!(INDEX_HTML.contains("chips"));
        assert!(!INDEX_HTML.contains("placeholder=\"channel id\""));
        assert!(!INDEX_HTML.contains("placeholder=\"role ids\""));
    }

    #[test]
    fn page_offers_all_write_policies() {
        for policy in ["admins", "roles", "disabled"] {
            assert!(INDEX_HTML.contains(policy), "missing policy {policy}");
        }
    }

    #[test]
    fn page_surfaces_missing_bot_token() {
        // A hub without a bot token must say so, not just look broken.
        assert!(INDEX_HTML.contains("bot_configured"));
        assert!(INDEX_HTML.contains("without a Discord bot token"));
    }
}
