use crate::settings::Settings;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::fs;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Snippet {
    pub id: String,
    pub source: String,
    pub trigger: String,
    pub description: String,
    pub body: String,
    pub regex: bool,
    pub automatic: bool,
    pub in_word: bool,
    pub line_start: bool,
    pub context: String,
    pub priority: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Catalog {
    pub snippets: Vec<Snippet>,
    pub global: String,
    pub warnings: Vec<String>,
}

pub fn load(settings: &Settings) -> Catalog {
    let mut catalog = Catalog::default();
    if settings.workshop_enabled {
        match workshop(include_str!("../assets/snippets/latex-workshop.json")) {
            Ok(snippets) => catalog.snippets.extend(snippets),
            Err(e) => catalog.warnings.push(e.to_string()),
        }
    }
    if settings.hsnips_enabled {
        if settings.hsnips_path.as_os_str().is_empty() {
            catalog
                .warnings
                .push("Choose your LaTeX .hsnips file in Settings to load HyperSnips.".into());
        } else {
            match fs::read_to_string(&settings.hsnips_path)
                .context("Could not read HyperSnips file")
                .and_then(|source| hypersnips(&source))
            {
                Ok(hs) => {
                    catalog.snippets.extend(hs.snippets);
                    catalog.global = hs.global;
                }
                Err(e) => catalog.warnings.push(format!("{e:#}")),
            }
        }
    }
    catalog
}

fn workshop(source: &str) -> Result<Vec<Snippet>> {
    let data: Value = serde_json::from_str(source)?;
    let mut result = vec![];
    for (name, value) in data
        .as_object()
        .context("Snippet catalog must be an object")?
    {
        let body = match &value["body"] {
            Value::String(body) => body.clone(),
            Value::Array(lines) => lines
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n"),
            _ => continue,
        };
        let prefixes: Vec<&str> = match &value["prefix"] {
            Value::String(prefix) => vec![prefix],
            Value::Array(prefixes) => prefixes.iter().filter_map(Value::as_str).collect(),
            _ => vec![name.as_str()],
        };
        for (i, prefix) in prefixes.iter().enumerate() {
            result.push(Snippet {
                id: format!("workshop:{name}:{i}"),
                source: "LaTeX Workshop".into(),
                trigger: prefix.to_string(),
                description: value["description"].as_str().unwrap_or(name).into(),
                body: body.clone(),
                regex: false,
                automatic: false,
                in_word: false,
                line_start: false,
                context: String::new(),
                priority: 0,
            });
        }
    }
    Ok(result)
}

fn quoted(input: &str) -> Result<(&str, &str)> {
    let quote = input.chars().next().context("Missing snippet trigger")?;
    if quote != '`' && quote != '"' {
        bail!("Expected a quoted snippet field: {input}");
    }
    let mut escaped = false;
    for (index, c) in input.char_indices().skip(1) {
        if c == quote && !escaped {
            return Ok((&input[1..index], input[index + 1..].trim()));
        }
        escaped = c == '\\' && !escaped;
    }
    bail!("Unterminated snippet field")
}

pub fn hypersnips(source: &str) -> Result<Catalog> {
    let mut result = Catalog::default();
    let mut lines = source.lines().enumerate().peekable();
    let mut context = String::new();
    let mut priority = 0;
    while let Some((number, raw)) = lines.next() {
        let line = raw.trim();
        if line == "global" {
            let mut body = vec![];
            let mut closed = false;
            for (_, raw) in lines.by_ref() {
                if raw.trim() == "endglobal" {
                    closed = true;
                    break;
                }
                body.push(raw);
            }
            if !closed {
                bail!("Unterminated HyperSnips global block");
            }
            result.global.push_str(&body.join("\n"));
            result.global.push('\n');
        } else if let Some(expression) = line.strip_prefix("context ") {
            context = expression.to_owned();
        } else if let Some(value) = line.strip_prefix("priority ") {
            priority = value.trim().parse().context("Invalid snippet priority")?;
        } else if let Some(header) = line.strip_prefix("snippet ") {
            let regex = header.starts_with('`');
            let (trigger, remaining) = if header.starts_with(['`', '"']) {
                quoted(header).with_context(|| format!("HyperSnips line {}", number + 1))?
            } else {
                let (trigger, rest) = header
                    .split_once(char::is_whitespace)
                    .unwrap_or((header, ""));
                (trigger, rest.trim())
            };
            let (description, flags) = if remaining.starts_with('"') {
                quoted(remaining)?
            } else {
                (trigger, remaining)
            };
            let mut body = vec![];
            let mut closed = false;
            for (_, raw) in lines.by_ref() {
                if raw.trim() == "endsnippet" {
                    closed = true;
                    break;
                }
                body.push(raw);
            }
            if !closed {
                bail!("Unterminated HyperSnips snippet at line {}", number + 1);
            }
            result.snippets.push(Snippet {
                id: format!("hsnips:{}", number + 1),
                source: "HyperSnips".into(),
                trigger: trigger.into(),
                description: description.into(),
                body: body.join("\n"),
                regex,
                automatic: flags.contains('A'),
                in_word: flags.contains('i'),
                line_start: flags.contains('b'),
                context: context.clone(),
                priority,
            });
            context.clear();
            priority = 0;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_workshop_and_hypersnips_metadata() -> Result<()> {
        let workshop = workshop(include_str!("../assets/snippets/latex-workshop.json"))?;
        assert_eq!(workshop.len(), 52);
        assert!(
            workshop
                .iter()
                .any(|s| s.trigger == "BEQ" && s.body.contains("\\begin{equation}"))
        );
        let source = "global\nconst identity = x => x;\nendglobal\npriority 20\ncontext math(context)\nsnippet `(?<!\\\\)sq` \"Square root\" iA\n\\\\sqrt{$1}$0\nendsnippet\nsnippet \"beg\" \"Environment\" b\nbody\nendsnippet";
        let parsed = hypersnips(source)?;
        assert_eq!(parsed.snippets.len(), 2);
        let first = &parsed.snippets[0];
        assert!(first.regex && first.automatic && first.in_word);
        assert_eq!(first.priority, 20);
        assert_eq!(first.context, "math(context)");
        assert!(parsed.snippets[1].context.is_empty());
        assert!(parsed.snippets[1].line_start);
        assert!(hypersnips("snippet `a` \"b\"\nunterminated").is_err());
        Ok(())
    }
}
