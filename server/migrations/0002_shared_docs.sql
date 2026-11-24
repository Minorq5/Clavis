-- Общие заметки. Сервер хранит только шифротекст: ключ документа есть лишь у участников.

alter table public.devices add column kx_pub text;
alter table public.devices add column display_name text not null default '';

create table public.shared_docs (
  id uuid primary key,
  owner_device uuid not null references public.devices(id) on delete cascade,
  epoch integer not null default 1,
  meta_ct text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table public.doc_members (
  doc_id uuid not null references public.shared_docs(id) on delete cascade,
  device_id uuid not null references public.devices(id) on delete cascade,
  role text not null check (role in ('owner', 'editor')),
  added_at timestamptz not null default now(),
  primary key (doc_id, device_id)
);
create index doc_members_device_idx on public.doc_members (device_id);

-- Ключ документа для каждой эпохи, зашифрованный открытым ключом X25519 участника.
create table public.doc_keys (
  doc_id uuid not null references public.shared_docs(id) on delete cascade,
  device_id uuid not null references public.devices(id) on delete cascade,
  epoch integer not null,
  wrapped text not null,
  primary key (doc_id, device_id, epoch)
);

create table public.doc_updates (
  id bigserial primary key,
  doc_id uuid not null references public.shared_docs(id) on delete cascade,
  epoch integer not null,
  sender uuid references public.devices(id) on delete set null,
  ct text not null,
  created_at timestamptz not null default now()
);
create index doc_updates_doc_idx on public.doc_updates (doc_id, id);

create table public.doc_snapshots (
  doc_id uuid primary key references public.shared_docs(id) on delete cascade,
  epoch integer not null,
  upto_id bigint not null,
  ct text not null,
  created_at timestamptz not null default now()
);

-- Приглашение: хэш секрета и ключ документа, зашифрованный ключом из секрета.
create table public.doc_invites (
  token_hash text primary key,
  doc_id uuid not null references public.shared_docs(id) on delete cascade,
  epoch integer not null,
  wrapped text not null,
  created_by uuid not null references public.devices(id) on delete cascade,
  expires_at timestamptz not null,
  used_at timestamptz,
  used_by uuid references public.devices(id) on delete set null
);
create index doc_invites_doc_idx on public.doc_invites (doc_id);

-- Изображения лежат на сервере, пока их не скачают все участники.
create table public.doc_blobs (
  id uuid primary key,
  doc_id uuid not null references public.shared_docs(id) on delete cascade,
  epoch integer not null,
  ct text not null,
  pending uuid[] not null,
  created_at timestamptz not null default now()
);
create index doc_blobs_doc_idx on public.doc_blobs (doc_id);

alter table public.shared_docs enable row level security;
alter table public.doc_members enable row level security;
alter table public.doc_keys enable row level security;
alter table public.doc_updates enable row level security;
alter table public.doc_snapshots enable row level security;
alter table public.doc_invites enable row level security;
alter table public.doc_blobs enable row level security;

revoke all on public.shared_docs, public.doc_members, public.doc_keys, public.doc_updates,
  public.doc_snapshots, public.doc_invites, public.doc_blobs from anon, authenticated;

create or replace function public.accept_invite(p_hash text, p_doc uuid, p_device uuid)
returns table (epoch integer, wrapped text)
language plpgsql security definer set search_path = public as $$
declare
  inv public.doc_invites%rowtype;
begin
  update public.doc_invites
     set used_at = now(), used_by = p_device
   where token_hash = p_hash and doc_id = p_doc and used_at is null and expires_at > now()
  returning * into inv;
  if not found then
    return;
  end if;
  insert into public.doc_members (doc_id, device_id, role)
  values (p_doc, p_device, 'editor')
  on conflict (doc_id, device_id) do nothing;
  return query select inv.epoch, inv.wrapped;
end;
$$;

create or replace function public.ack_blob(p_blob uuid, p_device uuid)
returns void
language sql security definer set search_path = public as $$
  update public.doc_blobs set pending = array_remove(pending, p_device) where id = p_blob;
  delete from public.doc_blobs where id = p_blob and cardinality(pending) = 0;
$$;

revoke execute on function public.accept_invite(text, uuid, uuid) from public, anon, authenticated;
revoke execute on function public.ack_blob(uuid, uuid) from public, anon, authenticated;
