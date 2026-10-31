<script lang="ts">
  import { onMount } from 'svelte';

  type Message = { id: string; sequence: number; label: string; text: string; time: number; verified: boolean; envelope_hex: string; body_hex: string };
  type Node = { name: string; stage: string; public_key: string; messages: number; sqlite: string };
  let node: Node | null = $state(null);
  let messages: Message[] = $state([]);
  let label = $state('');
  let text = $state('');
  let loading = $state(true);
  let saving = $state(false);
  let error = $state('');
  let notice = $state('');
  let secure = $state(false);
  let retry: { id: string; label: string; text: string } | null = null;

  async function json<T>(path: string, options?: RequestInit): Promise<T> {
    const response = await fetch(path, { cache: 'no-store', ...options });
    if (!response.ok) {
      let reason = 'The node could not complete that request. Please try again.';
      try { reason = (await response.json()).error || reason; } catch { /* Keep a readable fallback. */ }
      throw new Error(reason);
    }
    return response.json();
  }
  async function refresh() {
    loading = true; error = '';
    try {
      const result = await Promise.all([json<Node>('/api/node'), json<Message[]>('/api/messages')]);
      [node, messages] = result;
    } catch (e) { error = e instanceof Error ? e.message : 'The local node is unavailable.'; }
    finally { loading = false; }
  }
  async function send(event: SubmitEvent) {
    event.preventDefault();
    if (saving) return;
    saving = true; error = ''; notice = '';
    const cleanLabel = label.trim(); const cleanText = text.trim();
    if (!retry || retry.label !== cleanLabel || retry.text !== cleanText) {
      retry = { id: crypto.randomUUID(), label: cleanLabel, text: cleanText };
    }
    try {
      const message = await json<Message>('/api/messages', { method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ request_id: retry.id, label: cleanLabel, text: cleanText }) });
      messages = [message, ...messages.filter(m => m.id !== message.id)].slice(0, 100);
      if (node) node = { ...node, messages: Math.max(node.messages, message.sequence) };
      text = ''; retry = null; notice = 'Message saved and verified.';
    } catch (e) { error = e instanceof Error ? e.message : 'The message could not be saved. Your draft is still here.'; }
    finally { saving = false; }
  }
  function date(seconds: number) {
    return new Date(seconds * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  }
  function download(message: Message) {
    const blob = new Blob([JSON.stringify({ format: 'bbsbox.m00.export/v0', public_key: node?.public_key, ...message }, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a'); anchor.href = url; anchor.download = `bbsbox-message-${message.sequence}.json`; anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  onMount(() => { secure = window.isSecureContext; refresh(); });
</script>

<svelte:head><title>BBSBOX · Foundation</title><meta name="description" content="BBSBOX development prototype for local messages and signature verification." /></svelte:head>

<div class="shell">
  <aside aria-label="Node navigation">
    <a class="brand" href="#home" aria-label="BBSBOX home"><span class="mark" aria-hidden="true"><i></i><i></i><i></i><i></i></span>BBSBOX<span class="brand-dot">.</span></a>
    <div class="node-label"><span class="tiny-label">LOCAL NODE</span><strong>Foundation lab</strong><span class="subtle">M00 prototype</span></div>
    <nav><a class="selected" href="#home"><span aria-hidden="true">⌂</span> Overview <span class="nav-dot"></span></a><a href="#messagebook"><span aria-hidden="true">☷</span> Messagebook</a><a href="#about"><span aria-hidden="true">◇</span> About this build</a></nav>
    <div class="sidebar-bottom"><span class="lab-tag">M00 / PROTOTYPE</span></div>
  </aside>
  <main id="home">
    <header><div class="breadcrumb">BBSBOX <span>/</span> Foundation</div><div class="connection"><span class:online={node !== null}></span>{node ? 'Local node connected' : loading ? 'Finding the local node…' : 'Node unavailable'}</div></header>
    <section class="hero" aria-labelledby="welcome">
      <h1 id="welcome">Test node</h1>
      <p>Post and read local test messages.</p>
    </section>
    <div class="stats" aria-label="Foundation status">
      <div><span class="stat-icon cyan" aria-hidden="true">↔</span><div><strong>Local storage</strong><span>Stored on this node.</span></div></div>
      <div><span class="stat-icon yellow" aria-hidden="true">✳</span><div><strong>{node?.messages ?? '—'} {node?.messages === 1 ? 'message' : 'messages'} saved</strong><span>Signed by this test node.</span></div></div>
      <div><span class="stat-icon pink" aria-hidden="true">◇</span><div><strong>{secure ? 'Secure connection' : 'Checking connection'}</strong><span>{secure ? 'Browser secure context active.' : 'Use the local HTTPS address.'}</span></div></div>
    </div>
    <section id="messagebook" class="message-section" aria-labelledby="message-heading">
      <div class="section-title"><div><h2 id="message-heading">Messagebook</h2></div><button class="refresh" onclick={refresh} disabled={loading || saving} aria-label="Refresh messages"><span aria-hidden="true">↻</span> {loading ? 'Loading…' : 'Refresh'}</button></div>
      <div class="message-grid">
        <div class="compose panel">
          <div class="panel-heading"><span class="square cyan-bg"></span><h3>New message</h3></div>
          <form onsubmit={send}>
            <label for="name">Your name <span>Test label · visible to visitors</span></label>
            <input id="name" bind:value={label} maxlength="48" required placeholder="Name" autocomplete="nickname" />
            <div class="label-row"><label for="message">Your message</label><span>{text.length}/2,000</span></div>
            <textarea id="message" bind:value={text} maxlength="2000" required rows="5" placeholder="Write a message."></textarea>
            <button class="send" type="submit" disabled={saving || !node || !secure || !label.trim() || !text.trim()}>{saving ? 'Saving here…' : 'Post message'}<span aria-hidden="true">↗</span></button>
            <p class="form-footnote">Messages are public on this test node.</p>
          </form>
          <div class="feedback" aria-live="polite">{#if error}<p class="error" role="alert">{error}</p>{/if}{#if notice}<p class="success">{notice}</p>{/if}</div>
        </div>
        <div class="message-list panel" aria-busy={loading}>
          <div class="panel-heading"><span class="square yellow-bg"></span><h3>Messages</h3><span class="list-count">{messages.length}{node && node.messages > 100 ? ' latest' : ''}</span></div>
          {#if loading && !messages.length}<div class="empty"><span class="empty-symbol" aria-hidden="true">···</span><h4>Loading messages…</h4></div>
          {:else if !messages.length}<div class="empty"><span class="empty-symbol" aria-hidden="true">✳</span><h4>No messages yet.</h4><p>Posted messages appear here.</p></div>
          {:else}<ol>{#each messages as message (message.id)}<li><article><div class="message-top"><span class="avatar" aria-hidden="true">{message.label.slice(0,1).toUpperCase()}</span><div><h4>{message.label}</h4><time datetime={new Date(message.time * 1000).toISOString()}>{date(message.time)}</time></div><span class="verified">✓ Node verified</span></div><p class="message-body">{message.text}</p><details><summary>Verification details</summary><p>This node’s fixture key signed this message. A name label does not prove personal identity.</p><dl><dt>Local sequence</dt><dd>{message.sequence}</dd><dt>Event ID</dt><dd>{message.id}</dd></dl><button class="export" onclick={() => download(message)}>Download signed record ↓</button></details></article></li>{/each}</ol>{/if}
        </div>
      </div>
    </section>
    <section id="about" class="about" aria-labelledby="about-heading"><div><h2 id="about-heading">Prototype status</h2><p>Local messages are available. Account sign-in, encrypted forums, and mesh exchange are not yet connected to this interface.</p><details><summary>Technical details</summary><p>The node signs test messages and verifies their signatures when reading them. Display names are test labels, not authenticated accounts.</p>{#if node}<dl><dt>Node fixture public key</dt><dd>{node.public_key}</dd><dt>Storage</dt><dd>SQLite {node.sqlite} · WAL / FULL</dd></dl>{/if}<p><a href="/local-ca.pem" download="bbsbox-local-ca.pem">Download the local test CA</a> only after comparing its fingerprint with the node operator. This page cannot independently establish trust in its own host.</p></details></div></section>
    <footer><span>BBSBOX<span class="brand-dot">.</span></span><span>M00 development prototype</span></footer>
  </main>
</div>

<style>
  :global(*){box-sizing:border-box} :global(html){scroll-behavior:smooth;scroll-padding-top:28px} :global(body){margin:0;background:#08090b;color:#f3f5f7;font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;font-size:14px;line-height:1.5} :global(button),:global(input),:global(textarea){font:inherit} :global(button),:global(a),:global(summary){-webkit-tap-highlight-color:transparent} :global(a){color:inherit;text-decoration:none} :global(button){cursor:pointer} :global(button:disabled){cursor:default;opacity:.5} :global(:focus-visible){outline:2px solid #66f2dc;outline-offset:4px} :global(::selection){background:#66f2dc;color:#08090b}
  .shell{max-width:1540px;margin:auto;display:grid;grid-template-columns:236px minmax(0,1fr);min-height:100vh} aside{padding:34px 23px;border-right:1px solid #24262a;display:flex;flex-direction:column;position:sticky;top:0;height:100vh} .brand{font-size:26px;letter-spacing:-1.5px;font-weight:850;display:flex;align-items:center;gap:10px}.brand-dot{color:#66f2dc}.mark{display:grid;grid-template-columns:repeat(2,8px);gap:3px;transform:rotate(-5deg);margin-right:2px}.mark i{width:8px;height:8px;background:#66f2dc}.mark i:last-child{background:#f4e784}.brand>.brand-dot{margin-left:-9px}.node-label{margin-top:54px;padding:0 10px;display:grid;gap:6px}.tiny-label{font-size:10px;font-weight:650;letter-spacing:1.6px;color:#9b9da7}.node-label strong{font-size:15px;font-weight:600}.subtle{color:#848894;font-size:12px}nav{margin-top:30px;display:grid;gap:7px}nav a{display:flex;align-items:center;gap:13px;border-radius:7px;padding:11px 12px;color:#9b9faa;font-size:12px;font-weight:550}nav a>span:first-child{font-size:20px;line-height:1}nav a.selected{background:#182522;color:#79f1dc}nav a:hover{background:#15191a;color:#f3f5f7}.nav-dot{margin-left:auto;background:#66f2dc;width:5px;height:5px;border-radius:50%}.sidebar-bottom{margin-top:auto;padding:28px 10px 0}.lab-tag{font-size:9px;letter-spacing:1.2px;border:1px solid #3a3c40;border-radius:4px;padding:6px 8px;color:#babec8}
  main{min-width:0;padding:0 48px}header{height:89px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid #24262a;gap:16px}.breadcrumb{font-size:11px;font-weight:550;color:#a0a4ae}.breadcrumb span{margin:0 11px;color:#484c54}.connection{font-size:10px;color:#a1a6af;display:flex;align-items:center;gap:7px}.connection>span{width:6px;height:6px;border-radius:50%;background:#707580}.connection>span.online{background:#77e2bc}.hero{padding:28px 0}h1{font-size:36px;line-height:1.2;letter-spacing:-1px;margin:0 0 10px;font-weight:650}.hero>p{color:#9b9fa9;font-size:14px;line-height:1.8}
  .stats{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:16px;border-top:1px solid #24262a;border-bottom:1px solid #24262a;padding:23px 0}.stats>div{display:flex;align-items:center;gap:12px}.stat-icon{font-size:27px;line-height:1}.cyan{color:#7cf3dc}.yellow{color:#f4e784}.pink{color:#f1a9df}.stats strong{display:block;font-size:11px;font-weight:600}.stats div div>span{display:block;font-size:10px;color:#9398a4;margin-top:3px}.message-section{padding:38px 0}.section-title{display:flex;align-items:flex-end;justify-content:space-between;gap:16px;margin-bottom:22px}h2{font-size:23px;letter-spacing:-.8px;font-weight:600;margin:7px 0 0}.refresh{background:transparent;border:1px solid #333840;border-radius:5px;color:#c2c7d0;padding:7px 11px;font-size:11px;display:flex;align-items:center;gap:7px}.refresh:hover{background:#171b20}.refresh>span{font-size:16px}.message-grid{display:grid;grid-template-columns:minmax(270px,.9fr) minmax(0,1.25fr);gap:22px;align-items:start}.panel{border:1px solid #2b2e35;border-radius:10px;background:#101216;overflow:hidden}.compose{padding:23px}.panel-heading{display:flex;align-items:center;gap:9px}.square{width:6px;height:6px}.cyan-bg{background:#7cf3dc}.yellow-bg{background:#f4e784}h3{margin:0;font-size:13px;font-weight:600;letter-spacing:-.1px}label{display:block;font-size:11px;color:#dde0e6;margin:0 0 8px}label>span{font-size:9px;display:block;color:#8c939f;margin-top:3px}input,textarea{width:100%;background:#0a0c0f;color:#f0f4f6;border:1px solid #343840;border-radius:5px;padding:11px 12px;font-size:12px}input::placeholder,textarea::placeholder{color:#747c89}input:focus,textarea:focus{border-color:#79dec8;outline:0;box-shadow:0 0 0 1px #79dec8}.label-row{display:flex;justify-content:space-between;margin-top:19px}.label-row>span{font-size:9px;color:#9198a4}textarea{resize:vertical;min-height:120px;line-height:1.7}.send{margin-top:18px;display:flex;align-items:center;justify-content:space-between;width:100%;border:0;background:#7cf3dc;color:#0b1512;padding:12px 14px;border-radius:5px;font-weight:650;font-size:12px}.send:hover:not(:disabled){background:#a4ffe8}.send>span{font-size:18px;line-height:1}.form-footnote{font-size:9px;color:#9198a4;text-align:center;margin:11px 0 0;line-height:1.6}.feedback p{margin:12px 0 0;font-size:11px}.error{color:#ffb4bd}.success{color:#9cf2c6}.message-list{min-height:430px}.message-list>.panel-heading{padding:23px;border-bottom:1px solid #272b31}.list-count{margin-left:auto;font-size:10px;color:#8e97a3}.empty{text-align:center;padding:48px 14px 40px}.empty-symbol{font-size:42px;color:#f4e784;line-height:1;display:block}.empty h4{font-size:17px;font-weight:500;letter-spacing:-.3px;margin:20px 0 9px}.empty p{color:#969daa;font-size:12px;line-height:1.8}ol{list-style:none;margin:0;padding:0;max-height:650px;overflow:auto}li+li{border-top:1px solid #292d34}article{padding:23px}.message-top{display:flex;align-items:center;gap:9px}.avatar{width:30px;height:30px;border-radius:8px;background:#1c3230;color:#8df2dc;display:grid;place-items:center;font-size:12px;font-weight:650}.message-top h4{margin:0;font-size:12px;font-weight:600}.message-top time{font-size:9px;color:#949ba8}.verified{margin-left:auto;font-size:8px;color:#a3e8bd;background:#1a2c22;border-radius:4px;padding:4px 6px}.message-body{font-size:13px;line-height:1.8;white-space:pre-wrap;overflow-wrap:anywhere;margin:18px 0;color:#d1d6df}details{font-size:10px;color:#a0a9b7}summary{cursor:pointer;min-height:24px}details p{line-height:1.8;max-width:660px}dl{font-size:10px}dt{color:#929cab;margin-top:9px}dd{margin:4px 0 0;overflow-wrap:anywhere;color:#d1d8e4;font-family:ui-monospace,monospace}.export{border:1px solid #3e4a4b;background:none;color:#a8ecd7;border-radius:4px;padding:6px 9px;font-size:10px}.about{padding:24px;display:flex;border:1px solid #2b2e35;border-radius:8px;background:#101216;margin:0 0 37px}.about h2{font-size:15px;letter-spacing:-.3px;margin:0;font-weight:550}.about>div>p{font-size:11px;line-height:1.8;color:#a8aa9e;margin:7px 0 10px;max-width:590px}.about a{color:#aff5dc;text-decoration:underline}footer{border-top:1px solid #25292e;padding:22px 0 28px;display:flex;justify-content:space-between;gap:18px;align-items:center}footer>span:first-child{font-size:14px;font-weight:700;letter-spacing:-.6px}footer>span:last-child{font-size:8px;letter-spacing:1px;color:#959da8}
  @media(min-width:1440px){main{padding:0 65px}.hero{padding:28px 0}}@media(max-width:1090px){.shell{grid-template-columns:205px minmax(0,1fr)}aside{padding:28px 16px}main{padding:0 28px}.message-grid{gap:15px;grid-template-columns:1fr 1.1fr}.compose{padding:19px}.stats{gap:10px}.stats>div{gap:8px}.stats strong{font-size:10px}.stats div div>span{font-size:9px}}
  @media(max-width:800px){.shell{display:block}aside{position:static;height:auto;border-right:0;border-bottom:1px solid #24262a;padding:20px 23px;display:block}.brand{font-size:24px}.node-label,.sidebar-bottom{display:none}nav{display:flex;gap:7px;margin-top:18px;flex-wrap:wrap}nav a{font-size:11px;padding:8px 11px}.nav-dot{margin-left:8px}main{padding:0 24px}header{height:62px}.hero{padding:28px 0}.message-grid{grid-template-columns:1fr 1.15fr}.stats{padding:19px 0}.message-section{padding:29px 0}.about{padding:24px;display:flex;border:1px solid #2b2e35;border-radius:8px;background:#101216;margin:0 0 37px}footer{padding-bottom:23px}}
  @media(max-width:600px){aside{padding:19px 18px}main{padding:0 18px}header{gap:10px}.breadcrumb{font-size:10px}.connection{font-size:9px}.hero>p{font-size:12px;max-width:260px}.stats{grid-template-columns:1fr;gap:15px;padding:20px 0}.stats>div{gap:13px}.stat-icon{width:24px;text-align:center;font-size:24px}.stats strong{font-size:11px}.stats div div>span{font-size:10px}.message-grid{grid-template-columns:1fr}.section-title h2{font-size:22px}.compose{padding:22px}.message-list{min-height:350px}.empty{padding:35px 12px}.about{padding:24px;display:flex;border:1px solid #2b2e35;border-radius:8px;background:#101216;margin:0 0 37px}footer{align-items:flex-start;flex-direction:column;gap:7px}.about h2{font-size:14px}nav a{gap:7px;padding:8px 9px}nav a>span:first-child{font-size:17px}.nav-dot{display:none}}

  form{margin-top:20px}
  @media(prefers-reduced-motion:reduce){:global(html){scroll-behavior:auto}}
</style>
