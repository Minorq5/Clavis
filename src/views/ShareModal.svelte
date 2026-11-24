<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { api, type SharedMember } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { colorFor } from "../lib/collab";
  import { ask } from "../lib/dialog.svelte";
  import { toast } from "../lib/toast.svelte";
  import Icon from "../lib/Icon.svelte";
  import Modal from "../lib/Modal.svelte";
  import { sharedError } from "../lib/sharedError";

  let { id, role, kind = "note", onclose, onleft }: { id: string; role: "owner" | "editor"; kind?: "note" | "snippets"; onclose: () => void; onleft: () => void } = $props();

  let members = $state<SharedMember[]>([]);
  let link = $state("");
  let busy = $state(false);
  let copied = $state(false);

  onMount(async () => {
    members = await api.sharedMembers(id).catch(() => []);
  });

  async function invite() {
    busy = true;
    try {
      link = await api.sharedInvite(id);
      copied = false;
    } catch (e) {
      toast(sharedError(e));
    } finally {
      busy = false;
    }
  }

  async function copy() {
    await api.copyText(link);
    copied = true;
  }

  async function remove(m: SharedMember) {
    if (!(await ask({ title: t("sh.removeQ", { name: m.name || "?" }), text: t("sh.removeConfirm", { name: m.name || "?" }), ok: t("sh.remove"), danger: true }))) return;
    try {
      await api.sharedRemove(id, m.device);
      members = members.filter((x) => x.device !== m.device);
      link = "";
    } catch (e) {
      toast(sharedError(e));
    }
  }

  async function leave() {
    const owner = role === "owner";
    const ok = await ask({
      title: owner ? (kind === "snippets" ? t("sn.deleteSetQ") : t("sh.deleteQ")) : t("sh.leaveQ"),
      text: owner ? t("sh.deleteConfirm") : t("sh.leaveConfirm"),
      ok: owner ? t("sh.delete") : t("sh.leave"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.sharedLeave(id);
      onleft();
    } catch (e) {
      toast(sharedError(e));
    }
  }

  const initials = (name: string) => name.trim().split(/\s+/).slice(0, 2).map((w) => w[0]?.toUpperCase() ?? "").join("") || "?";
</script>

<Modal title={kind === "snippets" ? t("sn.shareTitle") : t("sh.inviteTitle")} {onclose} width={500}>
  <div class="dlg">
    {#if role === "owner"}
      <p class="muted">{kind === "snippets" ? t("sn.inviteHelp") : t("sh.inviteHelp")}</p>
      {#if link}
        <div class="linkbox" in:fade={{ duration: 120 }}>
          <code>{link}</code>
          <button class="btn" onclick={copy}><Icon name={copied ? "check" : "copy"} />{copied ? t("common.copied") : t("sh.copyLink")}</button>
        </div>
      {:else}
        <button class="btn primary self" onclick={invite} disabled={busy}><Icon name="link" />{t("sh.inviteMake")}</button>
      {/if}
    {/if}

    <h3>{t("sh.members")}</h3>
    <ul class="members">
      {#each members as m (m.device)}
        <li>
          <span class="av" style="background:{colorFor(m.device)}">{initials(m.name)}</span>
          <span class="mname">{m.name || "—"}{#if m.me}{" "}<span class="faint">({t("sh.you")})</span>{/if}</span>
          <span class="faint role">{m.role === "owner" ? t("sh.owner") : t("sh.editor")}</span>
          {#if role === "owner" && !m.me}
            <button class="btn ghost danger" onclick={() => remove(m)}>{t("sh.remove")}</button>
          {/if}
        </li>
      {/each}
    </ul>

    <div class="foot">
      <button class="btn ghost danger" onclick={leave}>{role === "owner" ? t("sh.delete") : t("sh.leave")}</button>
    </div>
  </div>
</Modal>

<style>
  .dlg { display: grid; gap: 12px; }
  .dlg p { margin: 0; line-height: 1.55; font-size: var(--t-s); }
  .self { justify-self: start; }
  .linkbox { display: grid; gap: 8px; padding: 10px 12px; border: 1px solid var(--text); border-radius: var(--r-s); }
  .linkbox code { font: 11.5px/1.5 var(--mono); word-break: break-all; user-select: all; color: var(--text); }
  .linkbox .btn { justify-self: start; }
  h3 { margin: 10px 0 0; font: 600 var(--t-s)/1.3 var(--font); color: var(--muted); }
  .members { list-style: none; margin: 0; padding: 0; display: grid; }
  .members li { display: flex; align-items: center; gap: 10px; padding: 8px 0; border-bottom: 1px solid var(--line); }
  .av { width: 22px; height: 22px; border-radius: 50%; display: inline-grid; place-items: center; font: 600 9.5px/1 var(--font); color: #151514; flex: none; }
  .mname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .role { font-size: var(--t-xs); }
  .members .btn { height: 26px; font-size: var(--t-xs); }
  .foot { border-top: 1px solid var(--line); padding-top: 12px; }
  .foot .btn { margin-left: -12px; }
</style>
