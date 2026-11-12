//! Turning instance names into disk names (decision 0008): safe on Windows and Unix, unique among siblings
//! (ignoring case), and never clashing with forge's own files or with a property in the parent's `_.toml`.
//! A disk name doesn't have to turn back into the instance name: when they differ, a `name =` line keeps the real one.

use std::collections::HashSet;

/// Keys forge uses itself inside a property file; a child can't have these names there.
pub const RESERVED: [&str; 4] = ["class", "name", "tags", "attributes"];

/// Escaped names longer than this are cut and given a hash, to stay inside path limits.
const MAX_KEY: usize = 100;
const CUT_TO: usize = 80;

/// Endings forge gives its own file names (extensions, and the `.server` / `.client` of script files, decision 0020);
/// a name ending in one is escaped so it can't clash with them.
const OWN_SUFFIXES: [&str; 5] = [".luau", ".rbxm", ".toml", ".server", ".client"];

/// The stem of a script folder's own file (`init.luau`); a child can't be called that, in any case.
const INIT: &str = "init";

const DEVICE_NAMES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

fn pct(b: u8) -> String {
    format!("%{b:02X}")
}

/// A name's disk form before siblings are considered.
/// `clashes` says whether a key is already used in the parent's property file (a property of its class).
pub fn base(name: &str, clashes: &dyn Fn(&str) -> bool) -> String {
    if name.is_empty() {
        return "~0".into();
    }
    let mut out = String::new();
    for c in name.chars() {
        let bad = matches!(
            c,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '%' | '~'
        ) || (c as u32) < 0x20
            || c as u32 == 0x7f;
        if bad {
            out.push_str(&pct(c as u8))
        } else {
            out.push(c)
        }
    }
    // Windows drops a trailing space or dot, so they're escaped.
    if let Some(last) = out.chars().last().filter(|c| *c == ' ' || *c == '.') {
        out.pop();
        out.push_str(&pct(last as u8));
    }
    let lower = out.to_ascii_lowercase();
    if let Some(suffix) = OWN_SUFFIXES.iter().find(|s| lower.ends_with(*s)) {
        let dot = out.len() - suffix.len();
        out.replace_range(dot..dot + 1, "%2E");
    }
    // `_` and `init` are the stems of a folder's own files (`_.toml`, `_.rbxm`, `init.luau`), so a child can't be
    // called either.
    if out == "_"
        || out.eq_ignore_ascii_case(INIT)
        || is_device_name(&out)
        || RESERVED.contains(&out.as_str())
        || clashes(&out)
    {
        let first = out.chars().next().unwrap_or('_');
        let rest: String = out.chars().skip(1).collect();
        out = format!(
            "{}{rest}",
            if first.is_ascii() {
                pct(first as u8)
            } else {
                first.to_string()
            }
        );
    }
    if out.len() > MAX_KEY {
        let mut cut = CUT_TO;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out = format!("{}~h{:016x}", &out[..cut], fnv(name.as_bytes()));
    }
    out
}

fn is_device_name(s: &str) -> bool {
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    DEVICE_NAMES.contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
}

/// Disk names for a group of siblings, in their order. `pins` are names they had on disk before (if any): a pin is
/// kept when it's still a valid name for that sibling and nobody else has it. The rest get their base name, or
/// `~2`, `~3`... in `order` (content order, so the same tree always gives the same names).
pub fn assign(bases: &[String], pins: &[Option<String>], order: &[usize]) -> Vec<String> {
    let mut keys: Vec<Option<String>> = vec![None; bases.len()];
    let mut taken: HashSet<String> = HashSet::new();
    for i in 0..bases.len() {
        if let Some(pin) = &pins[i]
            && fits(pin, &bases[i])
            && taken.insert(pin.to_lowercase())
        {
            keys[i] = Some(pin.clone());
        }
    }
    for &i in order {
        if keys[i].is_some() {
            continue;
        }
        let mut n = 1;
        loop {
            let candidate = if n == 1 {
                bases[i].clone()
            } else {
                format!("{}~{n}", bases[i])
            };
            if taken.insert(candidate.to_lowercase()) {
                keys[i] = Some(candidate);
                break;
            }
            n += 1;
        }
    }
    keys.into_iter().map(|k| k.unwrap_or_default()).collect()
}

/// Whether `pin` is `base` or `base~N` (N ≥ 2).
fn fits(pin: &str, base: &str) -> bool {
    pin == base
        || pin
            .strip_prefix(base)
            .and_then(|r| r.strip_prefix('~'))
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n >= 2)
}

/// Escapes one name in a path outside the tree (`@/Workspace/Arena`): only `%` and `/` need it, and it reverses.
pub fn outside(name: &str) -> String {
    name.replace('%', "%25").replace('/', "%2F")
}

pub fn outside_back(segment: &str) -> String {
    // One pass, so an escaped `%` is never read again as the start of another escape.
    let mut out = String::new();
    let mut rest = segment;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let code = rest.get(i + 1..i + 3).unwrap_or("");
        match code.to_ascii_uppercase().as_str() {
            "25" => out.push('%'),
            "2F" => out.push('/'),
            _ => {
                out.push('%');
                rest = &rest[i + 1..];
                continue;
            }
        }
        rest = &rest[i + 3..];
    }
    out.push_str(rest);
    out
}

/// FNV-1a, for stable hashes with no dependency (content order, long names).
pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(name: &str) -> String {
        base(name, &|k| k == "Text")
    }

    #[test]
    fn file_system_edge_cases() {
        assert_eq!(b("Frame"), "Frame");
        assert_eq!(b("Shared.v2"), "Shared.v2");
        assert_eq!(b("a/b:c"), "a%2Fb%3Ac");
        assert_eq!(b("50%"), "50%25");
        assert_eq!(b("dots..."), "dots..%2E");
        assert_eq!(b("space "), "space%20");
        assert_eq!(b("CON"), "%43ON");
        assert_eq!(b("nul.txt"), "%6Eul.txt");
        assert_eq!(b("COM1.luau"), "COM1%2Eluau"); // no longer a device name once the dot is escaped
        assert_eq!(b("Main.luau"), "Main%2Eluau");
        assert_eq!(b("_.toml"), "_%2Etoml");
        assert_eq!(b(""), "~0");
        assert_eq!(b("tab\there"), "tab%09here");
        assert_eq!(b("ünï"), "ünï");
    }

    #[test]
    fn names_that_read_as_script_files_are_escaped() {
        assert_eq!(b("init"), "%69nit");
        assert_eq!(b("Init"), "%49nit");
        assert_eq!(b("INIT"), "%49NIT");
        assert_eq!(b("Foo.server"), "Foo%2Eserver");
        assert_eq!(b("Foo.Client"), "Foo%2EClient");
        assert_eq!(b("init.server"), "init%2Eserver");
        assert_eq!(b("initial"), "initial");
        assert_eq!(b("Server"), "Server");
    }

    #[test]
    fn keys_used_in_the_property_file_are_escaped() {
        assert_eq!(b("Text"), "%54ext");
        assert_eq!(b("class"), "%63lass");
        assert_eq!(b("attributes"), "%61ttributes");
        assert_eq!(b("text"), "text");
    }

    #[test]
    fn long_names_are_cut_with_a_hash() {
        let long = "x".repeat(300);
        let k = b(&long);
        assert!(k.len() < 110 && k.contains("~h"), "{k}");
        assert_ne!(k, b(&"x".repeat(301)));
    }

    #[test]
    fn repeats_get_numbers_and_case_doesnt_matter() {
        let bases: Vec<String> = ["Strip", "strip", "Strip", "Pad"]
            .map(String::from)
            .to_vec();
        let keys = assign(&bases, &[None, None, None, None], &[2, 0, 1, 3]);
        assert_eq!(keys, ["Strip~2", "strip~3", "Strip", "Pad"]);
    }

    #[test]
    fn pins_are_kept_when_valid() {
        let bases: Vec<String> = ["Strip", "Strip"].map(String::from).to_vec();
        let keys = assign(
            &bases,
            &[Some("Strip~2".into()), Some("Strip".into())],
            &[0, 1],
        );
        assert_eq!(keys, ["Strip~2", "Strip"]);
        let stale = assign(&bases, &[Some("Other".into()), None], &[0, 1]);
        assert_eq!(stale, ["Strip", "Strip~2"]);
    }

    #[test]
    fn outside_paths_reverse() {
        for n in ["a/b", "50%", "%2F", "plain"] {
            assert_eq!(outside_back(&outside(n)), n);
        }
    }
}
