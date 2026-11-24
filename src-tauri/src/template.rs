//! Переменные в сниппетах. Правила те же, что в интерфейсе (src/lib/vars.ts):
//! {дата} и {время} подставляются сами, {буфер} — текст из буфера,
//! остальные имена спрашиваются; {{ и }} дают буквальные скобки.

use std::collections::HashMap;

enum Part<'a> {
    Text(&'a str),
    Brace(char),
    Var(&'a str),
}

fn parse(body: &str) -> Vec<Part<'_>> {
    let mut out = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("{{") {
            out.push(Part::Brace('{'));
            rest = r;
            continue;
        }
        if let Some(r) = rest.strip_prefix("}}") {
            out.push(Part::Brace('}'));
            rest = r;
            continue;
        }
        if rest.starts_with('{') {
            if let Some(end) = rest[1..].find(['}', '{', '\n']) {
                let inner = &rest[1..1 + end];
                if rest[1 + end..].starts_with('}') && !inner.is_empty() && inner.chars().count() <= 40 {
                    out.push(Part::Var(inner));
                    rest = &rest[2 + end..];
                    continue;
                }
            }
        }
        let first = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        let next = rest[first..].find(['{', '}']).map(|i| i + first).unwrap_or(rest.len());
        out.push(Part::Text(&rest[..next]));
        rest = &rest[next..];
    }
    out
}

fn builtin(name: &str) -> Option<String> {
    let now = chrono_like::now();
    match name.to_lowercase().as_str() {
        "дата" => Some(format!("{:02}.{:02}.{}", now.day, now.month, now.year)),
        "date" => Some(format!("{:02}/{:02}/{}", now.day, now.month, now.year)),
        "время" | "time" => Some(format!("{:02}:{:02}", now.hour, now.minute)),
        _ => None,
    }
}

fn is_clipboard(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "буфер" | "clipboard")
}

pub fn asked(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in parse(body) {
        if let Part::Var(v) = p {
            let n = v.trim();
            if n.is_empty() || builtin(n).is_some() || is_clipboard(n) {
                continue;
            }
            if !out.iter().any(|x| x == n) {
                out.push(n.to_string());
            }
        }
    }
    out
}

pub fn fill(body: &str, values: &HashMap<String, String>, clipboard: &str) -> String {
    let mut s = String::with_capacity(body.len());
    for p in parse(body) {
        match p {
            Part::Text(t) => s.push_str(t),
            Part::Brace(c) => s.push(c),
            Part::Var(v) => {
                let n = v.trim();
                if let Some(b) = builtin(n) {
                    s.push_str(&b);
                } else if is_clipboard(n) {
                    s.push_str(clipboard);
                } else {
                    s.push_str(values.get(n).map(String::as_str).unwrap_or(""));
                }
            }
        }
    }
    s
}

/// Местное время без внешних зависимостей.
mod chrono_like {
    pub struct Now {
        pub year: i32,
        pub month: u32,
        pub day: u32,
        pub hour: u32,
        pub minute: u32,
    }

    pub fn now() -> Now {
        let offset = local_offset_seconds();
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
            + offset;
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Алгоритм Хиннанта: дни с 1970-01-01 в дату.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        Now { year: (if m <= 2 { y + 1 } else { y }) as i32, month: m, day: d, hour: (rem / 3600) as u32, minute: ((rem % 3600) / 60) as u32 }
    }

    #[cfg(windows)]
    fn local_offset_seconds() -> i64 {
        use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        unsafe {
            let mut tz = TIME_ZONE_INFORMATION::default();
            let r = GetTimeZoneInformation(&mut tz);
            let bias = tz.Bias + if r == 2 { tz.DaylightBias } else { tz.StandardBias };
            -(bias as i64) * 60
        }
    }

    #[cfg(not(windows))]
    fn local_offset_seconds() -> i64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vars() {
        let b = "Здравствуйте, {имя}! До {срок}. {{скобки}} {дата}";
        assert_eq!(asked(b), vec!["имя", "срок"]);
        let mut v = HashMap::new();
        v.insert("имя".to_string(), "Анна".to_string());
        v.insert("срок".to_string(), "пятницы".to_string());
        let out = fill(b, &v, "");
        assert!(out.starts_with("Здравствуйте, Анна! До пятницы. {скобки} "));
        assert_eq!(fill("{буфер}!", &HashMap::new(), "x"), "x!");
        assert_eq!(fill("a } b { c", &HashMap::new(), ""), "a } b { c");
    }
}
