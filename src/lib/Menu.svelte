<script lang="ts">
  import { tick } from "svelte";
  import { fade } from "svelte/transition";
  import { menu, closeMenu, type MenuItem } from "./menu.svelte";
  import Icon from "./Icon.svelte";

  let box = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });
  let i = $state(-1);

  const actionable = (it: MenuItem): it is Extract<MenuItem, { run: () => void }> => "run" in it && !it.disabled;

  $effect(() => {
    const m = menu.current;
    if (!m) return;
    i = -1;
    pos = { left: m.x, top: m.y };
    tick().then(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      const left = Math.max(8, Math.min(m.x, window.innerWidth - r.width - 8));
      const top = m.up ? Math.max(8, m.y - r.height) : Math.max(8, Math.min(m.y, window.innerHeight - r.height - 8));
      pos = { left, top };
      box.focus();
    });
  });

  function choose(it: MenuItem) {
    if (!actionable(it)) return;
    closeMenu();
    it.run();
  }

  function onkey(e: KeyboardEvent) {
    const m = menu.current;
    if (!m) return;
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(); return; }
    const idx = m.items.map((it, n) => (actionable(it) ? n : -1)).filter((n) => n >= 0);
    if (!idx.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const cur = idx.indexOf(i);
      i = idx[(cur + (e.key === "ArrowDown" ? 1 : idx.length - 1) + (cur < 0 && e.key === "ArrowUp" ? 1 : 0)) % idx.length];
    } else if (e.key === "Enter" && i >= 0) {
      e.preventDefault();
      choose(m.items[i]);
    }
  }
</script>

<svelte:window
  onmousedown={(e) => { if (menu.current && box && !box.contains(e.target as Node)) closeMenu(); }}
  onresize={closeMenu}
  onblur={closeMenu}
/>

{#if menu.current}
  <div class="menu" bind:this={box} tabindex="-1" role="menu" style="left:{pos.left}px; top:{pos.top}px" onkeydown={onkey} transition:fade={{ duration: 80 }}>
    {#each menu.current.items as it, n}
      {#if "sep" in it}<div class="sep" role="separator"></div>
      {:else if "header" in it}<div class="hd">{it.header}</div>
      {:else}
        <button role="menuitem" class:danger={it.danger} class:sel={n === i} disabled={it.disabled} onmousemove={() => (i = n)} onclick={() => choose(it)}>
          <span class="ic">{#if it.checked}<Icon name="check" size={14} />{:else if it.icon}<Icon name={it.icon} size={14} />{/if}</span>
          <span class="lb">{it.label}</span>
          {#if it.hint}<span class="hint">{it.hint}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .menu { position: fixed; z-index: 80; min-width: 200px; max-width: 320px; max-height: min(70vh, 520px); overflow-y: auto; padding: 4px; background: var(--raise); border: 1px solid var(--line-strong); border-radius: var(--r-m); box-shadow: 0 14px 36px rgba(0, 0, 0, 0.5); outline: none; }
  button { width: 100%; display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px 0 8px; border-radius: var(--r-s); text-align: left; color: var(--text); font-size: var(--t-s); }
  button.sel { background: var(--text); color: var(--bg); }
  button.danger { color: var(--danger); }
  button.danger.sel { background: var(--danger); color: #fff; }
  button:disabled { opacity: 0.4; }
  .ic { width: 16px; display: grid; place-items: center; color: var(--muted); flex: none; }
  button.sel .ic, button.danger .ic { color: inherit; }
  .lb { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .hint { color: var(--faint); font-size: var(--t-xs); flex: none; }
  button.sel .hint { color: var(--on-text-muted); }
  .sep { height: 1px; background: var(--line); margin: 4px 6px; }
  .hd { padding: 8px 10px 4px; font-size: var(--t-xs); color: var(--faint); }
</style>
