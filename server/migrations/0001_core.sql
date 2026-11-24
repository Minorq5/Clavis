-- Таблицы сервера Clavis. Все закрыты политиками RLS без разрешающих правил:
-- обращаться к ним может только серверная функция с ключом сервиса.

create table public.server_keys (
  id smallint primary key default 1 check (id = 1),
  sk text not null,
  pk text not null,
  created_at timestamptz not null default now()
);

create table public.owner (
  id smallint primary key default 1 check (id = 1),
  pubkey text not null,
  created_at timestamptz not null default now()
);

create table public.codes (
  id uuid primary key default gen_random_uuid(),
  label text not null default '',
  code_hash text not null unique,
  is_owner boolean not null default false,
  revoked boolean not null default false,
  created_at timestamptz not null default now(),
  used_at timestamptz
);

create table public.devices (
  id uuid primary key default gen_random_uuid(),
  pubkey text not null unique,
  hw_hash text not null,
  name text not null default '',
  label text not null default '',
  code_id uuid references public.codes(id) on delete set null,
  revoked boolean not null default false,
  activated_at timestamptz not null default now(),
  last_seen timestamptz not null default now()
);
create index devices_hw_idx on public.devices (hw_hash);

create table public.hw_attempts (
  hw_hash text primary key,
  failures integer not null default 0,
  blocked boolean not null default false,
  name text not null default '',
  updated_at timestamptz not null default now()
);

create table public.ip_attempts (
  ip text not null,
  window_start timestamptz not null,
  count integer not null default 0,
  primary key (ip, window_start)
);

create table public.requests (
  id uuid primary key default gen_random_uuid(),
  name text not null,
  message text not null default '',
  hw_hash text not null,
  status text not null default 'pending' check (status in ('pending', 'approved', 'declined')),
  created_at timestamptz not null default now()
);
create index requests_status_idx on public.requests (status, created_at desc);

create table public.used_nonces (
  nonce text primary key,
  at timestamptz not null default now()
);

alter table public.server_keys enable row level security;
alter table public.owner enable row level security;
alter table public.codes enable row level security;
alter table public.devices enable row level security;
alter table public.hw_attempts enable row level security;
alter table public.ip_attempts enable row level security;
alter table public.requests enable row level security;
alter table public.used_nonces enable row level security;

revoke all on all tables in schema public from anon, authenticated;

create or replace function public.register_failure(p_hw text, p_name text, p_limit integer)
returns table (failures integer, blocked boolean)
language sql security definer set search_path = public as $$
  insert into public.hw_attempts as h (hw_hash, failures, blocked, name, updated_at)
  values (p_hw, 1, 1 >= p_limit, p_name, now())
  on conflict (hw_hash) do update
    set failures = h.failures + 1,
        blocked = h.blocked or (h.failures + 1 >= p_limit),
        name = excluded.name,
        updated_at = now()
  returning h.failures, h.blocked;
$$;

create or replace function public.claim_code(p_hash text)
returns table (id uuid, label text, is_owner boolean)
language sql security definer set search_path = public as $$
  update public.codes set used_at = now()
   where code_hash = p_hash and used_at is null and not revoked
  returning id, label, is_owner;
$$;

create or replace function public.hit_ip(p_ip text, p_limit integer)
returns boolean
language sql security definer set search_path = public as $$
  with w as (select date_trunc('hour', now()) as ws),
  up as (
    insert into public.ip_attempts as a (ip, window_start, count)
    select p_ip, ws, 1 from w
    on conflict (ip, window_start) do update set count = a.count + 1
    returning a.count
  )
  select count <= p_limit from up;
$$;

revoke execute on function public.register_failure(text, text, integer) from public, anon, authenticated;
revoke execute on function public.claim_code(text) from public, anon, authenticated;
revoke execute on function public.hit_ip(text, integer) from public, anon, authenticated;
