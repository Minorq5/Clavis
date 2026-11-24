-- Отложенное удаление: устройство просит удалить свои данные, сервер стирает их через 48 часов.
alter table public.devices add column if not exists erase_at timestamptz;
