<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fade } from "svelte/transition";
  import { api, listen, type ClipItem, type Snippet, type Settings, type PickerMode } from "../lib/api";
  import { search } from "../lib/fuzzy";
  import { t, ago, i18n, type Key } from "../lib/i18n.svelte";
  import { apply, looksLikeCode, formatJson, type TransformId } from "../lib/transforms";
  import { askedVars, fill } from "../lib/vars";
  import { calc, formatNumber } from "../lib/calc";
  import Icon from "../lib/Icon.svelte";

  type PSnippet = Snippet & { team?: string };
  type ActKind = "tr" | "calc" | "translate" | "shorten" | "qr" | "ocr";
  type Act = { key: string; kind: ActKind; icon: string; label: string; sub: string; arg?: string };
  type Service = "translate" | "shorten";
  type Result = { act: Act; title: string; text?: string; image?: string; loading?: boolean; error?: string; consent?: Service };

  let { initialMode = "popup" as PickerMode } = $props();

  type Tab = "history" | "snippets";
  // svelte-ignore state_referenced_locally
  let mode = $state<PickerMode>(initialMode);
  let tab = $state<Tab>("history");
  let query = $state("");
  let history = $state<ClipItem[]>([]);
  let snippets = $state<PSnippet[]>([]);
  let settings = $state<Settings | null>(null);
  let paused = $state(false);
  let index = $state(0);
  let showTransforms = $state(false);
  let tIndex = $state(0);
  let form = $state<{ snippet: Snippet; names: string[]; values: Record<string, string>; paste: boolean; counted?: boolean } | null>(null);
  let opening = $state(0); // меняется при каждом показе окна, перезапускает анимацию
  let busy = $state(false);
  let hl = $state<{ top: number; height: number } | null>(null);
  let result = $state<Result | null>(null);
  let resultSeq = 0;

  let input = $state<HTMLInputElement>();
  let listEl = $state<HTMLElement>();

  const transforms: TransformId[] = ["plain", "tidy", "layout", "upper", "lower", "title", "json"];

  const itemText = (h: ClipItem) =>
    h.kind === "text" ? h.text ?? "" : h.kind === "files" ? (h.files ?? []).join(" ") : `${t("kind.image")} ${h.source ?? ""}`;

  const histFiltered = $derived(search(history, query, itemText));
  const pinned = $derived(histFiltered.filter((h) => h.pinned));
  const recent = $derived(histFiltered.filter((h) => !h.pinned));
  const histOrdered = $derived([...pinned, ...recent]);
  const snipFiltered = $derived(search(snippets, query, (s) => `${s.title} ${s.abbr} ${s.tags.join(" ")} ${s.body}`));
  const count = $derived(tab === "history" ? histOrdered.length : snipFiltered.length);
  const currentClip = $derived(tab === "history" ? histOrdered[index] : undefined);
  const currentSnip = $derived(tab === "snippets" ? snipFiltered[index] : undefined);
  const calcValue = $derived.by(() => {
    const txt = currentClip?.kind === "text" ? currentClip.text ?? "" : "";
    if (!txt || txt.length > 200) return null;
    const v = calc(txt.replace(/=\s*$/, ""));
    return v === null ? null : { shown: formatNumber(v, i18n.lang), raw: i18n.lang === "ru" ? String(v).replace(".", ",") : String(v) };
  });

  // Всё, что можно сделать с выбранной записью: преобразования и быстрые действия.
  const actions = $derived.by((): Act[] => {
    const c = currentClip;
    if (!c) return [];
    if (c.kind === "image") return [{ key: "ocr", kind: "ocr", icon: "scan", label: t("qa.ocr"), sub: "Windows" }];
    if (c.kind !== "text") return [];
    const txt = c.text ?? "";
    const out: Act[] = [];
    if (calcValue !== null) out.push({ key: "calc", kind: "calc", icon: "calc", label: "= " + calcValue.shown, sub: t("qa.calc"), arg: calcValue.raw });
    const cyr = /[а-яё]/i.test(txt);
    const langs: ("ru" | "en" | "bg")[] = cyr ? ["en", "bg", "ru"] : ["ru", "bg"];
    for (const l of langs) out.push({ key: "tl-" + l, kind: "translate", icon: "globe", label: t("qa.translate", { lang: t(`qa.lang.${l}` as Key) }), sub: "MyMemory", arg: l });
    if (isUrl(txt)) out.push({ key: "short", kind: "shorten", icon: "link", label: t("qa.shorten"), sub: "clck.ru" });
    if (txt.length <= 1200) out.push({ key: "qr", kind: "qr", icon: "qr", label: t("qa.qr"), sub: t("qa.qrHint") });
    for (const id of transforms) {
      if (id === "json" && formatJson(txt) === null) continue;
      out.push({ key: "tr-" + id, kind: "tr", icon: "wand", label: t(`tr.${id}` as Key), sub: oneLine(apply(id, txt)).slice(0, 90), arg: id });
    }
    return out;
  });

  async function load() {
    try {
      const [h, own, st, p, team] = await Promise.all([api.history(), api.snippets(), api.settings(), api.historyPaused(), api.teamSnippets().catch(() => [])]);
      history = h;
      settings = st;
      paused = p;
      snippets = [
        ...own,
        ...(team ?? []).map((x) => ({ id: `team:${x.setId}:${x.id}`, title: x.title, body: x.body, folder: null, tags: x.tags, abbr: x.abbr, updated: 0, team: x.setTitle })),
      ];
      if (settings) i18n.lang = settings.lang;
    } catch {
      history = [];
      snippets = [];
    }
  }

  async function openForm(snippetId: string) {
    const s = snippets.find((x) => x.id === snippetId);
    if (!s) return;
    tab = "snippets";
    await tick();
    const names = askedVars(s.body);
    // Сокращение уже засчитано ядром, второй раз не считаем.
    form = { snippet: s, names, values: Object.fromEntries(names.map((n) => [n, ""])), paste: true, counted: true };
    await tick();
    document.querySelector<HTMLInputElement>(".varform input")?.focus();
  }

  onMount(() => {
    load().then(() => input?.focus());
    const offs: Array<() => void> = [];
    listen<{ mode: PickerMode; snippet: string | null }>("picker:shown", async (p) => {
      mode = p.mode;
      query = "";
      index = 0;
      tab = "history";
      showTransforms = false;
      result = null;
      form = null;
      busy = false;
      opening++;
      await load();
      if (p.snippet) await openForm(p.snippet);
      else {
        await tick();
        input?.focus();
      }
    }).then((o) => offs.push(o));
    listen("history:changed", load).then((o) => offs.push(o));
    listen("settings:changed", load).then((o) => offs.push(o));
    return () => offs.forEach((o) => o());
  });

  $effect(() => {
    query;
    tab;
    index = 0;
    showTransforms = false;
    result = null;
    if (listEl) listEl.scrollTop = 0;
  });

  // Подсветка выбранной строки плавно переезжает к новой позиции.
  $effect(() => {
    index;
    tab;
    histOrdered;
    snipFiltered;
    paused;
    tick().then(() => {
      const el = listEl?.querySelector<HTMLElement>('[aria-selected="true"]');
      hl = el ? { top: el.offsetTop, height: el.offsetHeight } : null;
    });
  });

  async function scrollToCurrent() {
    await tick();
    listEl?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }

  function move(d: number) {
    if (!count) return;
    index = Math.max(0, Math.min(count - 1, index + d));
    showTransforms = false;
    result = null;
    scrollToCurrent();
  }

  function enterPastes() {
    return (settings?.enterAction ?? "paste") === "paste";
  }

  async function run(fn: () => Promise<void>) {
    if (busy) return;
    busy = true;
    try {
      await fn();
    } finally {
      busy = false;
    }
  }

  async function commit(opts: { plain?: boolean; invert?: boolean; transform?: TransformId } = {}) {
    const paste = enterPastes() !== !!opts.invert;
    const clip = currentClip;
    const snip = currentSnip;
    if (clip) {
      if (opts.transform && opts.transform !== "plain" && clip.text) {
        const text = apply(opts.transform, clip.text);
        await run(() => api.pasteText(text, paste));
      } else {
        await run(() => api.pasteItem(clip.id, !!opts.plain || opts.transform === "plain", paste));
      }
      return;
    }
    if (snip) {
      const names = askedVars(snip.body);
      if (names.length) {
        form = { snippet: snip, names, values: Object.fromEntries(names.map((n) => [n, ""])), paste };
        await tick();
        document.querySelector<HTMLInputElement>(".varform input")?.focus();
        return;
      }
      const clipText = history.find((h) => h.kind === "text")?.text ?? "";
      api.snippetUsed(snip.id).catch(() => {});
      await run(() => api.pasteText(fill(snip.body, {}, clipText), paste));
    }
  }

  function errorText(kind: ActKind, e: unknown): string {
    const s = String(e);
    if (s === "limit") return t("qa.limit");
    if (s === "too_large") return t("qa.tooLarge");
    if (s === "network") return t("qa.network");
    if (kind === "ocr") return t("qa.ocrUnavailable");
    return t("toast.error", { e: s });
  }

  async function runAct(act: Act, invert = false) {
    const clip = currentClip;
    if (!clip) return;
    const paste = enterPastes() !== invert;
    if (act.kind === "tr") return commit({ transform: act.arg as TransformId, invert });
    if (act.kind === "calc") return run(() => api.pasteText(act.arg ?? "", paste));
    const seq = ++resultSeq;
    result = { act, title: act.label, loading: true };
    try {
      if (act.kind === "qr") {
        const QR = (await import("qrcode")).default;
        const image = await QR.toDataURL(clip.text ?? "", { margin: 2, width: 512, errorCorrectionLevel: "M", color: { dark: "#000000", light: "#ffffff" } });
        if (seq === resultSeq) result = { act, title: act.label, image };
        return;
      }
      const text =
        act.kind === "translate" ? await api.translate(clip.text ?? "", act.arg as "ru" | "en" | "bg")
        : act.kind === "shorten" ? await api.shorten(clip.text ?? "")
        : await api.ocrItem(clip.id);
      if (seq !== resultSeq) return;
      result = text.trim() ? { act, title: act.kind === "ocr" ? t("qa.textDetected") : act.label, text } : { act, title: act.label, error: t("qa.noText") };
    } catch (e) {
      if (seq !== resultSeq) return;
      const s = String(e);
      if (s.startsWith("consent:")) result = { act, title: t("consent.title"), consent: s.slice(8) as Service };
      else result = { act, title: act.label, error: errorText(act.kind, e) };
    }
  }

  async function allowService(service: Service) {
    if (!result) return;
    const act = result.act;
    await api.consentGive(service);
    await runAct(act);
  }

  async function useResult(invert = false) {
    if (!result || result.loading) return;
    const paste = enterPastes() !== invert;
    if (result.consent) return allowService(result.consent);
    if (result.image) {
      const img = result.image;
      return run(() => api.pasteImage(img, paste));
    }
    if (result.text) {
      const text = result.text;
      return run(() => api.pasteText(text, paste));
    }
  }

  function openActions() {
    if (!actions.length) return;
    result = null;
    showTransforms = !showTransforms;
    tIndex = 0;
  }

  async function submitForm() {
    if (!form) return;
    const clipText = history.find((h) => h.kind === "text")?.text ?? "";
    const text = fill(form.snippet.body, form.values, clipText);
    const paste = form.paste;
    if (!form.counted) api.snippetUsed(form.snippet.id).catch(() => {});
    form = null;
    await run(() => api.pasteText(text, paste));
  }

  async function togglePin() {
    if (!currentClip) return;
    const id = currentClip.id;
    await api.historyPin(id, !currentClip.pinned);
    await load();
    const i = histOrdered.findIndex((h) => h.id === id);
    if (i >= 0) index = i;
    scrollToCurrent();
  }

  async function remove() {
    if (!currentClip) return;
    const keep = index;
    await api.historyDelete(currentClip.id);
    await load();
    index = Math.min(keep, Math.max(0, histOrdered.length - 1));
  }

  async function toSnippet() {
    if (!currentClip?.text) return;
    const firstLine = currentClip.text.trim().split("\n")[0].slice(0, 60);
    await api.snippetSave({ id: "", title: firstLine, body: currentClip.text, folder: null, tags: [], abbr: "", updated: 0 });
    await api.openMain("snippets");
  }

  function onKey(e: KeyboardEvent) {
    if (form) {
      if (e.key === "Escape") {
        e.preventDefault();
        form = null;
        input?.focus();
      }
      return;
    }
    const ctrl = e.ctrlKey || e.metaKey;
    if (result) {
      if (e.key === "Escape" || e.key === "ArrowLeft" || e.key === "Backspace") { e.preventDefault(); result = null; return; }
      if (e.key === "Enter") { e.preventDefault(); useResult(ctrl); return; }
      if (["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Tab"].includes(e.key)) { e.preventDefault(); return; }
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (showTransforms) showTransforms = false;
      else if (query) query = "";
      else api.hidePicker();
      return;
    }
    if (e.key === "Tab") {
      e.preventDefault();
      tab = tab === "history" ? "snippets" : "history";
      return;
    }
    if (showTransforms && actions.length) {
      const n = actions.length;
      if (e.key === "ArrowDown") { e.preventDefault(); tIndex = (tIndex + 1) % n; scrollAct(); return; }
      if (e.key === "ArrowUp") { e.preventDefault(); tIndex = (tIndex - 1 + n) % n; scrollAct(); return; }
      if (e.key === "Enter") { e.preventDefault(); runAct(actions[tIndex], ctrl); return; }
      if (e.key === "ArrowLeft") { e.preventDefault(); showTransforms = false; return; }
    }
    if (e.key === "ArrowDown") { e.preventDefault(); move(1); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); move(-1); return; }
    if (e.key === "PageDown") { e.preventDefault(); move(8); return; }
    if (e.key === "PageUp") { e.preventDefault(); move(-8); return; }
    if (e.key === "Enter") { e.preventDefault(); commit({ plain: e.shiftKey, invert: ctrl }); return; }
    if (ctrl && /^Digit[1-9]$/.test(e.code)) {
      e.preventDefault();
      const i = Number(e.code.slice(5)) - 1;
      if (i < count) {
        index = i;
        commit();
      }
      return;
    }
    if (ctrl && e.code === "KeyP") { e.preventDefault(); togglePin(); return; }
    if (ctrl && e.code === "KeyT" && actions.length) { e.preventDefault(); openActions(); return; }
    if (ctrl && e.code === "KeyS" && currentClip?.kind === "text") { e.preventDefault(); toSnippet(); return; }
    if (e.key === "Delete" && (!query || e.shiftKey) && tab === "history") { e.preventDefault(); remove(); return; }
    if (e.key === "ArrowRight" && actions.length && input?.selectionStart === query.length) {
      e.preventDefault();
      showTransforms = true;
      tIndex = 0;
    }
  }

  async function scrollAct() {
    await tick();
    document.querySelector<HTMLElement>('.transforms [aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }

  const oneLine = (s: string) => s.replace(/\s+/g, " ").trim();
  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const isColor = (s?: string) => !!s && /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(s.trim());
  const isUrl = (s?: string) => !!s && /^https?:\/\/\S+$/.test(s.trim());
  const enterLabel = $derived(enterPastes() ? t("hint.paste") : t("hint.copy"));
</script>

<svelte:window onkeydown={onKey} />

{#key opening}
  <div class="picker {mode}" class:busy>
    <header>
      <label class="search">
        <Icon name="search" />
        <input bind:this={input} bind:value={query} placeholder={t("app.search")} spellcheck="false" aria-label={t("app.search")} />
        {#if query}<button class="clear" aria-label={t("common.cancel")} onclick={() => { query = ""; input?.focus(); }}><Icon name="close" size={12} /></button>{/if}
      </label>
      <div class="tabs" role="tablist">
        <span class="tabhl" class:right={tab === "snippets"}></span>
        <button role="tab" aria-selected={tab === "history"} onclick={() => (tab = "history")}>{t("tab.history")}</button>
        <button role="tab" aria-selected={tab === "snippets"} onclick={() => (tab = "snippets")}>{t("tab.snippets")}</button>
      </div>
    </header>

    <div class="body">
      <div class="list" bind:this={listEl} role="listbox" aria-label={tab === "history" ? t("tab.history") : t("tab.snippets")}>
        {#if hl}<div class="hl" style="transform: translateY({hl.top}px); height: {hl.height}px"></div>{/if}

        {#if paused && tab === "history"}
          <button class="paused" onclick={async () => { await api.historyPause(false); paused = false; }}>
            <Icon name="pause" /><span>{t("picker.paused")}</span><Icon name="play" />
          </button>
        {/if}

        {#if tab === "history"}
          {#if !histOrdered.length}
            <p class="empty" in:fade={{ duration: 120 }}>{query ? t("picker.empty.search", { q: query }) : t("picker.empty.history")}</p>
          {/if}
          {#each histOrdered as h, i (h.id)}
            {#if i === 0 && pinned.length}<div class="group">{t("picker.pinned")}</div>{/if}
            {#if i === pinned.length && pinned.length && recent.length}<div class="group">{t("picker.recent")}</div>{/if}
            <button class="row" role="option" aria-selected={i === index} onmousemove={() => { if (index !== i) { index = i; showTransforms = false; } }} onclick={() => commit()}>
              <span class="glyph">
                {#if h.kind === "image"}<img src={h.image} alt="" />
                {:else if isColor(h.text)}<span class="swatch" style="background:{h.text}"></span>
                {:else}<Icon name={h.kind === "files" ? "files" : isUrl(h.text) ? "link" : "text"} />{/if}
              </span>
              <span class="line">
                {#if h.kind === "image"}{t("kind.image")} <span class="dim">{h.imageSize?.join("×")}</span>
                {:else if h.kind === "files"}{h.files?.map(fileName).join(", ")}
                {:else}{oneLine(h.text ?? "")}{/if}
              </span>
              {#if h.pinned}<span class="pinmark"><Icon name="pin" size={12} /></span>{/if}
              {#if i < 9}<span class="num">Ctrl {i + 1}</span>{/if}
              <span class="time">{ago(h.at)}</span>
            </button>
          {/each}
        {:else}
          {#if !snipFiltered.length}
            <p class="empty" in:fade={{ duration: 120 }}>{query ? t("picker.empty.search", { q: query }) : t("picker.empty.snippets")}</p>
          {/if}
          {#each snipFiltered as s, i (s.id)}
            <button class="row" role="option" aria-selected={i === index} onmousemove={() => (index = i)} onclick={() => commit()}>
              <span class="glyph"><Icon name={s.team ? "users" : "snippet"} /></span>
              <span class="line">{s.title}</span>
              {#if s.team}<span class="abbr team">{s.team}</span>{/if}
              {#if s.abbr}<span class="abbr">{settings?.expander.prefix}{s.abbr}</span>{/if}
              {#if i < 9}<span class="num">Ctrl {i + 1}</span>{/if}
            </button>
          {/each}
        {/if}
      </div>

      <aside class="preview">
        {#if form}
          <form class="varform" in:fade={{ duration: 120 }} onsubmit={(e) => { e.preventDefault(); submitForm(); }}>
            <h2>{form.snippet.title}</h2>
            <p class="formhint">{t("picker.fill")}</p>
            {#each form.names as n}
              <label>
                <span>{n}</span>
                <input class="field" bind:value={form.values[n]} />
              </label>
            {/each}
            <div class="formactions">
              <button type="button" class="btn ghost" onclick={() => { form = null; input?.focus(); }}>{t("common.cancel")}</button>
              <button class="btn primary">{t("picker.fillGo")}</button>
            </div>
          </form>
        {:else if currentClip && result}
          {#key result.act.key + (result.loading ? "l" : "r")}
            <div class="content result" in:fade={{ duration: 90 }}>
              <p class="rtitle"><Icon name={result.consent ? "shield" : result.act.icon} size={14} /><span>{result.title}</span></p>
              {#if result.loading}
                <p class="working"><span class="spin"></span>{t("qa.working")}</p>
              {:else if result.consent}
                <p class="consent">{t(result.consent === "translate" ? "consent.translate" : "consent.shorten")}</p>
                <div class="formactions start">
                  <button class="btn primary" onclick={() => result?.consent && allowService(result.consent)}>{t("consent.allow")}</button>
                  <button class="btn ghost" onclick={() => (result = null)}>{t("common.cancel")}</button>
                </div>
              {:else if result.error}
                <p class="rerror">{result.error}</p>
              {:else if result.image}
                <img class="qr" src={result.image} alt={t("qa.qr")} />
              {:else}
                <pre>{result.text}</pre>
              {/if}
            </div>
          {/key}
          <footer class="meta">
            <span class="src"><kbd>Esc</kbd> {t("qa.back")}</span>
            <span class="actions">
              {#if result.text || result.image}
                <button class="btn ghost" onclick={() => useResult()}><Icon name="check" /><span class="lbl">{t("qa.copyResult")}</span></button>
              {/if}
            </span>
          </footer>
        {:else if currentClip}
          {#key currentClip.id + showTransforms}
            <div class="content" in:fade={{ duration: 90 }}>
              {#if showTransforms && actions.length}
                <ul class="transforms" role="listbox" aria-label={t("qa.title")}>
                  {#each actions as a, i (a.key)}
                    {#if i > 0 && a.kind === "tr" && actions[i - 1].kind !== "tr"}<li class="sep" role="presentation"></li>{/if}
                    <li>
                      <button role="option" aria-selected={i === tIndex} onmousemove={() => (tIndex = i)} onclick={() => runAct(a)}>
                        <span class="tricon"><Icon name={a.icon} size={14} /></span>
                        <span class="trname">{a.label}</span>
                        <span class="trprev">{a.sub}</span>
                      </button>
                    </li>
                  {/each}
                </ul>
              {:else if currentClip.kind === "image"}
                <img class="big" src={currentClip.image} alt="" />
                <p class="dim imgsize">{currentClip.imageSize?.join(" × ")}</p>
              {:else if currentClip.kind === "files"}
                <ul class="files">
                  {#each currentClip.files ?? [] as f}<li><Icon name="files" /><span>{fileName(f)}</span><small>{f}</small></li>{/each}
                </ul>
              {:else}
                {#if calcValue !== null}<p class="calcline">= {calcValue.shown}</p>{/if}
                {#if isColor(currentClip.text)}<div class="colorbig" style="background:{currentClip.text}"></div>{/if}
                <pre class:code={looksLikeCode(currentClip.text ?? "")}>{formatJson(currentClip.text ?? "") ?? currentClip.text}</pre>
              {/if}
            </div>
          {/key}
          <footer class="meta">
            <span class="src">{currentClip.source ? t("picker.source", { app: currentClip.source }) : ""}</span>
            <span class="actions">
              {#if actions.length}
                <button class="btn ghost" class:on={showTransforms} title="Ctrl+T" onclick={openActions}><Icon name="wand" /><span class="lbl">{t("hint.actions")}</span></button>
              {/if}
              <button class="btn ghost" title="Ctrl+P" onclick={togglePin}><Icon name="pin" /><span class="lbl">{currentClip.pinned ? t("picker.unpin") : t("picker.pin")}</span></button>
              {#if currentClip.kind === "text"}<button class="btn ghost" title="Ctrl+S" onclick={toSnippet}><Icon name="snippet" /><span class="lbl">{t("picker.toSnippet")}</span></button>{/if}
              <button class="btn ghost" title="{t('hint.delete')}  Del" aria-label={t("hint.delete")} onclick={remove}><Icon name="trash" /></button>
            </span>
          </footer>
        {:else if currentSnip}
          {#key currentSnip.id}
            <div class="content" in:fade={{ duration: 90 }}>
              <h2 class="sniptitle">{currentSnip.title}</h2>
              <pre class:code={looksLikeCode(currentSnip.body)}>{#each currentSnip.body.split(/(\{[^{}\n]{1,40}\})/) as part}{#if /^\{.*\}$/.test(part)}<mark>{part}</mark>{:else}{part}{/if}{/each}</pre>
            </div>
          {/key}
          {#if currentSnip.tags.length || currentSnip.abbr || currentSnip.team}
            <footer class="meta">
              <span class="src">{currentSnip.team ? t("sn.fromSet", { t: currentSnip.team }) + "  " : ""}{currentSnip.tags.map((x) => "#" + x).join("  ")}</span>
              {#if currentSnip.abbr}<span class="src">{settings?.expander.prefix}{currentSnip.abbr}</span>{/if}
            </footer>
          {/if}
        {/if}
      </aside>
    </div>

    <footer class="hints">
      <span><kbd>Enter</kbd>{enterLabel}</span>
      <span><kbd>Shift</kbd><kbd>Enter</kbd>{t("hint.plain")}</span>
      {#if tab === "history"}
        <span class="opt"><kbd>Ctrl</kbd><kbd>T</kbd>{t("hint.actions")}</span>
        <span class="opt"><kbd>Ctrl</kbd><kbd>P</kbd>{t("hint.pin")}</span>
      {/if}
      <span class="right"><kbd>Tab</kbd>{tab === "history" ? t("tab.snippets") : t("tab.history")}</span>
    </footer>
  </div>
{/key}

<style>
  .picker {
    height: 100%;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    grid-template-columns: minmax(0, 1fr);
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-m);
    overflow: hidden;
    animation: open 140ms var(--ease);
  }
  .picker.panel { border-radius: 0; border-width: 0 0 0 1px; animation-name: slide; animation-duration: 180ms; }
  .picker.busy { cursor: progress; }
  @keyframes open { from { opacity: 0; transform: translateY(-4px) scale(0.992); } }
  @keyframes slide { from { opacity: 0.4; transform: translateX(16px); } }

  header { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-bottom: 1px solid var(--line); }
  .search { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; color: var(--faint); cursor: text; }
  .search input { flex: 1; min-width: 0; background: none; border: 0; font-size: var(--t-l); color: var(--text); padding: 4px 0; user-select: text; }
  .search input::placeholder { color: var(--faint); }
  .search input:focus-visible { outline: none; }
  .clear { color: var(--faint); padding: 4px; border-radius: var(--r-s); display: grid; }
  .clear:hover { color: var(--text); background: var(--raise); }

  .tabs { position: relative; display: grid; grid-template-columns: 1fr 1fr; padding: 2px; border-radius: var(--r-s); background: var(--sunk); box-shadow: inset 0 0 0 1px var(--line); flex: none; }
  .tabhl { position: absolute; top: 2px; bottom: 2px; left: 2px; width: calc(50% - 2px); border-radius: 3px; background: var(--raise); box-shadow: inset 0 0 0 1px var(--line-strong); transition: transform 160ms var(--ease); }
  .tabhl.right { transform: translateX(100%); }
  .tabs button { position: relative; padding: 4px 12px; color: var(--muted); font-size: var(--t-s); transition: color 120ms var(--ease); }
  .tabs button:hover, .tabs button[aria-selected="true"] { color: var(--text); }

  .body { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); min-height: 0; }
  .panel .body { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1.25fr) minmax(0, 1fr); }

  .list { position: relative; overflow-y: auto; padding: 6px; border-right: 1px solid var(--line); }
  .panel .list { border-right: 0; border-bottom: 1px solid var(--line); }
  .hl { position: absolute; left: 6px; right: 6px; top: 0; border-radius: var(--r-s); background: var(--text); transition: transform 90ms var(--ease), height 90ms var(--ease); pointer-events: none; }

  .group { position: relative; padding: 10px 8px 4px; font-size: var(--t-xs); color: var(--faint); }

  .row { position: relative; width: 100%; display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 8px; border-radius: var(--r-s); text-align: left; color: var(--text); transition: color 60ms linear; }
  .row[aria-selected="true"] { color: var(--bg); }
  .row[aria-selected="true"] :is(.time, .num, .abbr, .dim, .glyph, .pinmark) { color: var(--on-text-muted); }

  .glyph { width: 18px; height: 18px; display: grid; place-items: center; color: var(--muted); flex: none; }
  .glyph img { width: 18px; height: 18px; object-fit: cover; border-radius: 2px; }
  .swatch { width: 12px; height: 12px; border-radius: 2px; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18); }
  .line { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { color: var(--faint); font-size: var(--t-s); }
  .time { color: var(--faint); font-size: var(--t-xs); flex: none; min-width: 34px; text-align: right; }
  .pinmark { color: var(--faint); display: grid; flex: none; }
  .num { color: var(--faint); font-size: 10.5px; flex: none; display: none; }
  .row[aria-selected="true"] .num { display: inline; }
  .abbr { color: var(--faint); font-size: var(--t-s); flex: none; }

  .empty { color: var(--muted); padding: 28px 12px; margin: 0; max-width: 36ch; line-height: 1.55; }
  .paused { position: relative; display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px; margin-bottom: 4px; border-radius: var(--r-s); color: var(--muted); border: 1px dashed var(--line-strong); font-size: var(--t-s); }
  .paused span { flex: 1; text-align: left; }
  .paused:hover { color: var(--text); border-color: var(--muted); }

  .preview { display: grid; grid-template-rows: minmax(0, 1fr) auto; min-height: 0; min-width: 0; }
  .content { overflow: auto; padding: 14px 16px; min-height: 0; }
  pre { margin: 0; white-space: pre-wrap; word-break: break-word; font: inherit; line-height: 1.55; user-select: text; }
  pre.code { font-family: var(--mono); font-size: 12px; line-height: 1.6; }
  pre :global(mark) { background: none; color: var(--text); box-shadow: inset 0 -1px 0 var(--muted); }
  .sniptitle { font-size: var(--t-m); font-weight: 600; margin: 0 0 10px; }
  .big { max-width: 100%; max-height: 100%; border-radius: var(--r-s); display: block; }
  .imgsize { margin: 8px 0 0; font-size: var(--t-xs); }
  .colorbig { height: 64px; border-radius: var(--r-s); margin-bottom: 12px; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1); }

  .files { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
  .files li { display: grid; grid-template-columns: 18px 1fr; column-gap: 8px; color: var(--muted); }
  .files span { color: var(--text); word-break: break-all; }
  .files small { grid-column: 2; color: var(--faint); font-size: var(--t-xs); word-break: break-all; }

  .transforms { list-style: none; margin: 0 -6px; padding: 0; display: grid; gap: 1px; }
  .transforms button { width: 100%; text-align: left; padding: 7px 10px; border-radius: var(--r-s); display: grid; gap: 1px; transition: background 80ms linear; }
  .transforms button[aria-selected="true"] { background: var(--raise); box-shadow: inset 2px 0 0 var(--text); }
  .transforms button { grid-template-columns: 18px minmax(0, 1fr); column-gap: 8px; align-items: center; }
  .tricon { grid-row: span 2; color: var(--faint); display: grid; }
  .transforms button[aria-selected="true"] .tricon { color: var(--text); }
  .transforms .sep { height: 1px; background: var(--line); margin: 4px 10px; }
  .trname { font-size: var(--t-s); }
  .trprev { font-size: var(--t-xs); color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .calcline { margin: 0 0 10px; font-family: var(--mono); font-size: var(--t-l); color: var(--text); }
  .result { display: grid; align-content: start; gap: 12px; }
  .rtitle { margin: 0; display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: var(--t-s); }
  .working { margin: 0; display: flex; align-items: center; gap: 10px; color: var(--muted); }
  .spin { width: 12px; height: 12px; border-radius: 50%; border: 1.5px solid var(--line-strong); border-top-color: var(--text); animation: spin 700ms linear infinite; }
  @keyframes spin { to { transform: rotate(1turn); } }
  .consent { margin: 0; line-height: 1.55; max-width: 44ch; }
  .rerror { margin: 0; color: var(--muted); line-height: 1.55; max-width: 44ch; }
  .qr { width: min(100%, 220px); image-rendering: pixelated; border-radius: var(--r-s); display: block; }
  .formactions.start { justify-content: flex-start; }
  .abbr.team { padding: 1px 6px; border-radius: 3px; box-shadow: inset 0 0 0 1px var(--line-strong); font-size: var(--t-xs); }
  kbd { font: inherit; }
  .meta { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 8px 6px 16px; border-top: 1px solid var(--line); color: var(--faint); font-size: var(--t-xs); min-width: 0; }
  .src { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .actions { display: flex; gap: 2px; flex: none; }
  .actions .btn { height: 26px; padding: 0 8px; font-size: var(--t-xs); }
  .actions .btn.on { color: var(--text); background: var(--raise); }

  .varform { padding: 16px; display: grid; gap: 12px; align-content: start; overflow-y: auto; }
  .varform h2 { margin: 0; font-size: var(--t-m); font-weight: 600; }
  .formhint { margin: -6px 0 0; color: var(--faint); font-size: var(--t-s); }
  .varform label { display: grid; gap: 4px; font-size: var(--t-s); color: var(--muted); }
  .formactions { display: flex; justify-content: flex-end; gap: 6px; }

  .hints { display: flex; gap: 16px; align-items: center; padding: 7px 12px; border-top: 1px solid var(--line); font-size: var(--t-xs); color: var(--faint); white-space: nowrap; overflow: hidden; }
  .hints span { display: inline-flex; gap: 4px; align-items: center; }
  .hints .right { margin-left: auto; }

  /* Узкое окно: панель справа или маленький экран. */
  .panel .hints .opt, .panel .actions .lbl { display: none; }
  @media (max-width: 640px) {
    .body { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1.3fr) minmax(0, 1fr); }
    .list { border-right: 0; border-bottom: 1px solid var(--line); }
    .hints .opt, .actions .lbl { display: none; }
  }
  @media (max-height: 360px) {
    .hints { display: none; }
  }
</style>
