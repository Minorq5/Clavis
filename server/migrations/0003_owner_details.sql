-- Последний IP-адрес устройства и запроса доступа: виден только владельцу в панели управления.
alter table public.devices add column if not exists last_ip text not null default '';
alter table public.hw_attempts add column if not exists last_ip text not null default '';
alter table public.requests add column if not exists ip text not null default '';
