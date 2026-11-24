<script lang="ts">
  import { onMount, tick } from "svelte";

  // Скрытое окно печати: получает HTML заметки, показывает его на белой странице и сообщает ядру, что можно печатать в PDF.
  let html = $state("");
  let title = $state("");

  onMount(() => {
    document.documentElement.classList.add("print");
    let off: (() => void) | undefined;
    (async () => {
      const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const { emit } = await import("@tauri-apps/api/event");
      const win = getCurrentWebviewWindow();
      off = await win.listen<{ html: string; title: string }>("print:html", async (e) => {
        html = e.payload.html;
        title = e.payload.title;
        document.title = title;
        await tick();
        await Promise.all([...document.images].map((img) => (img.complete ? null : new Promise((r) => { img.onload = img.onerror = r; }))));
        await document.fonts.ready;
        requestAnimationFrame(() => emit(`print:rendered:${win.label}`));
      });
      await emit(`print:ready:${win.label}`);
    })();
    return () => off?.();
  });
</script>

<article class="paper">{@html html}</article>

<style>
  :global(html.print), :global(html.print body) { background: #fff !important; color: #1b1b1a !important; height: auto !important; overflow: visible !important; }
  .paper { max-width: 720px; margin: 0 auto; padding: 0; font: 11pt/1.6 "Segoe UI", system-ui, sans-serif; color: #1b1b1a; user-select: text; }
  .paper :global(h1) { font-size: 20pt; line-height: 1.25; margin: 0 0 0.6em; font-weight: 600; }
  .paper :global(h2) { font-size: 14pt; margin: 1.4em 0 0.4em; font-weight: 600; break-after: avoid; }
  .paper :global(h3) { font-size: 12pt; margin: 1.2em 0 0.3em; font-weight: 600; break-after: avoid; }
  .paper :global(p) { margin: 0.35em 0; }
  .paper :global(ul), .paper :global(ol) { padding-left: 1.4em; margin: 0.4em 0; }
  .paper :global(ul.tasks) { list-style: none; padding-left: 0.1em; }
  .paper :global(ul.tasks li) { display: flex; gap: 8px; }
  .paper :global(.box) { width: 10pt; height: 10pt; border: 1px solid #777; border-radius: 2px; margin-top: 0.3em; flex: none; }
  .paper :global(li.done .box) { background: #333; }
  .paper :global(li.done > div) { color: #888; text-decoration: line-through; }
  .paper :global(blockquote) { margin: 0.6em 0; padding-left: 12px; border-left: 2px solid #ccc; color: #555; }
  .paper :global(pre) { background: #f4f4f2; border: 1px solid #e2e2de; border-radius: 4px; padding: 10px 12px; font: 9.5pt/1.55 "JetBrains Mono", Consolas, monospace; white-space: pre-wrap; break-inside: avoid; }
  .paper :global(code) { font-family: "JetBrains Mono", Consolas, monospace; font-size: 0.9em; }
  .paper :global(mark) { background: #f3e9b8; }
  .paper :global(hr) { border: 0; border-top: 1px solid #ddd; margin: 1.4em 0; }
  .paper :global(img) { max-width: 100%; break-inside: avoid; }
  .paper :global(table) { border-collapse: collapse; width: 100%; margin: 0.6em 0; break-inside: avoid; }
  .paper :global(td), .paper :global(th) { border: 1px solid #ccc; padding: 5px 8px; text-align: left; vertical-align: top; }
  .paper :global(th) { background: #f2f2ef; }
  .paper :global(.deadline) { padding: 0 5px; border: 1px solid #ccc; border-radius: 3px; font-size: 0.9em; white-space: nowrap; }
  @page { margin: 18mm 18mm 20mm; }
</style>
