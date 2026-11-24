-- Плановое удаление устаревших данных по срокам из политики конфиденциальности.
-- Выполните в SQL Editor проекта Supabase один раз: задание будет запускаться каждый час.
create extension if not exists pg_cron;

create or replace function public.clavis_retention() returns void language sql security definer set search_path = public as $$
  delete from public.requests r using public.devices d where d.erase_at <= now() and r.hw_hash = d.hw_hash;
  delete from public.hw_attempts h using public.devices d where d.erase_at <= now() and h.hw_hash = d.hw_hash and not h.blocked;
  delete from public.devices where erase_at <= now();
  delete from public.devices where revoked and last_seen < now() - interval '12 months';
  delete from public.hw_attempts where not blocked and updated_at < now() - interval '12 months';
  delete from public.requests where status <> 'pending' and created_at < now() - interval '30 days';
  delete from public.requests where status = 'pending' and created_at < now() - interval '90 days';
  delete from public.ip_attempts where window_start < now() - interval '2 hours';
  delete from public.doc_blobs where created_at < now() - interval '30 days';
  delete from public.doc_invites where expires_at < now() - interval '1 day';
$$;

revoke execute on function public.clavis_retention() from public, anon, authenticated;

select cron.schedule('clavis-retention', '17 * * * *', 'select public.clavis_retention()');
