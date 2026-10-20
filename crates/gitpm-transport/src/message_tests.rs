/// String literals of `source` outside comments and the test module.
fn literals(source: &str) -> Vec<String> {
    let code = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
    let chars: Vec<char> = code.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            // A char literal ('"', '\'', 'x'); a lifetime has no closing quote.
            '\'' if chars.get(i + 1) == Some(&'\\') => i += 4,
            '\'' if chars.get(i + 2) == Some(&'\'') => i += 3,
            'r' if chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'"') => {
                let start = i + 3;
                i = start;
                while !(chars[i] == '"' && chars.get(i + 1) == Some(&'#')) {
                    i += 1;
                }
                out.push(chars[start..i].iter().collect());
                i += 2;
            }
            '"' => {
                let mut text = String::new();
                i += 1;
                while chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    text.push(chars[i]);
                    i += 1;
                }
                out.push(text);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Library messages are neutral: no CLI command, flag or hosting service (the CLI
/// adds command hints per error kind). Git commands quoted in messages (`git …`) and
/// the README a new package repository gets are not messages.
#[test]
fn messages_name_no_cli_command() {
    let sources = [
        ("transport.rs", include_str!("transport.rs")),
        ("lib.rs", include_str!("lib.rs")),
        ("error.rs", include_str!("error.rs")),
    ];
    let commands = [
        "init",
        "fork",
        "remote",
        "auth",
        "info",
        "catalog",
        "commit",
        "install",
        "list",
        "check",
        "update",
        "rollback",
        "enable",
        "disable",
        "forget",
        "restore",
        "uninstall",
        "replace",
        "sync-dist",
        "register",
    ];
    let mut found = Vec::new();
    for (file, source) in sources {
        for text in literals(source) {
            // Git arguments (`--format=…`) and quoted git commands are not prose.
            let flag = text.contains(' ')
                && !text.starts_with("git ")
                && !text.starts_with("--")
                && text
                    .match_indices("--")
                    .any(|(at, _)| text[at + 2..].starts_with(|c: char| c.is_ascii_lowercase()));
            // `gitpm restore`, `` `restore ``, or advice such as "use restore"/"run update".
            let command = commands.iter().any(|c| {
                text.contains(&format!("gitpm {c}"))
                    || text.contains(&format!("`{c}"))
                    || ["use ", "run "].iter().any(|verb| {
                        text.match_indices(&format!("{verb}{c}")).any(|(at, m)| {
                            !text[at + m.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric())
                        })
                    })
            });
            if flag || command || text.contains("R2") || text.contains("catalog-update") {
                found.push(format!("{file}: {text}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "messages naming CLI commands:\n{}",
        found.join("\n")
    );
    assert!(literals("x(\"a --b\", '\"', r#\"c\"#) // \"d\"").len() == 2);
    // The checks catch the advice the messages used to give.
    let flagged = |text: &str| {
        commands.iter().any(|c| {
            ["use ", "run "].iter().any(|verb| {
                text.match_indices(&format!("{verb}{c}")).any(|(at, m)| {
                    !text[at + m.len()..].starts_with(|ch: char| ch.is_ascii_alphanumeric())
                })
            })
        })
    };
    assert!(flagged("kept; use restore, or uninstall it first"));
    assert!(!flagged("use the default branch") && !flagged("use restored files"));
}
