use postgres::{Client, NoTls};
use std::{env, process};

const DATABASE_URL: &str = "DATABASE_URL";

fn indent(text: &str, indent: &str) -> String {
    text.replace('\n', &format!("\n{}", indent))
}

struct ErrorSnippetInput {
    block_code: String,
    function_name: String,
    error_message: String,
    error_code: String,
    position: i32,
}

fn over_write_error_message_with_snippet(error: ErrorSnippetInput) -> String {
    let code_indent = 2;
    let position = error.position as usize;

    let mut line_no = 1;
    let mut column = 0;
    let mut index = 0;
    while index < position {
        column += 1;
        let char = error.block_code.chars().nth(index).unwrap();
        if char == '\n' {
            line_no += 1;
            column = 0;
        }
        index += 1;
    }

    let mut end_of_line = error.block_code[position..].find('\n').unwrap_or(0);
    end_of_line += if end_of_line > 0 { position } else { 0 };

    let previous_newline = error.block_code[..position].rfind('\n').unwrap_or(1) - 1;
    let previous_newline2 = if previous_newline == 0 {
        0
    } else {
        error.block_code[..previous_newline]
            .rfind('\n')
            .unwrap_or(0)
    };
    let previous_newline3 = if previous_newline2 == 0 {
        0
    } else {
        error.block_code[..previous_newline2]
            .rfind('\n')
            .unwrap_or(0)
    };
    let previous_newline4 = if previous_newline3 == 0 {
        0
    } else {
        error.block_code[..previous_newline3]
            .rfind('\n')
            .unwrap_or(0)
    };
    let start_of_line = if previous_newline > 0 {
        previous_newline + 1
    } else {
        0
    };

    let position_within_line = position - start_of_line;
    let mut snippet = "\x1b[31m|  \x1b[0m".to_string();
    snippet += &error
        .block_code
        .get(previous_newline4..end_of_line)
        .unwrap_or("")
        .replace('\t', " ");

    let lines = vec![
        format!(
            "\x1b[1;31m🛑 Error occurred at line {}, column {} of \"{}\":\x1b[0m",
            line_no, column, error.function_name
        ),
        indent(&indent(&snippet, "  "), "\x1b[31m| \x1b[0m"),
        format!(
            "\x1b[31m| {}\x1b[0m",
            "-".repeat(position_within_line - 1 + code_indent - 1) + "^"
        ),
        format!(
            "\x1b[31m| \x1b[0m\x1b[1;31m{}\x1b[0m\x1b[31m: {}\x1b[0m",
            error.error_code, error.error_message
        ),
    ];

    lines.join("\n")
}

struct Error {
    stack: String,
    message: String,
    severity: String,
    error_code: String,
    detail: String,
    hint: String,
    error_message_with_snippet: String,
}

fn log_database_error(error: Error) {
    let mut messages = vec!["".to_string()];

    if !error.error_message_with_snippet.is_empty() {
        messages.push(error.error_message_with_snippet);
    } else {
        messages.push("🛑 Error occurred whilst processing".to_string());
        messages.push(indent(&error.stack, "    "));
    }

    messages.push("".to_string());
    if !error.severity.is_empty() {
        messages.push(format!("Severity:\t{}", error.severity));
    }
    if !error.error_code.is_empty() {
        messages.push(format!("Code:    \t{}", error.error_code));
    }
    if !error.detail.is_empty() {
        messages.push(format!("Detail:  \t{}", error.detail));
    }
    if !error.hint.is_empty() {
        messages.push(format!("Hint:    \t{}", error.hint));
    }

    eprintln!("{}", messages.join("\n"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if the DATABASE_URL environment variable is set, otherwise exit
    let database_url = env::var(DATABASE_URL).unwrap_or_else(|_| {
        eprintln!(
            "plpgsql_check-cli: \x1b[1;31m🚨 {} environment variable is not set!\x1b[0m",
            DATABASE_URL
        );
        process::exit(1);
    });
    let mut client = Client::connect(&database_url, NoTls)?;

    let result = client.query(
        r#"
        select (pcf).functionid::regprocedure::text,
               (pcf).lineno,
               (pcf).statement,
               (pcf).sqlstate,
               (pcf).message,
               (pcf).detail,
               (pcf).hint,
               (pcf).level,
               (pcf)."position",
               (pcf).query,
               (pcf).context
        from ( select public.plpgsql_check_function_tb(pg_proc.oid, coalesce(pg_trigger.tgrelid, 0)) as pcf
               from pg_proc
                   left join pg_trigger
                            on (pg_trigger.tgfoid = pg_proc.oid)
               where prolang = ( select lang.oid from pg_language lang where lang.lanname = 'plpgsql' )
                 and pronamespace <> ( select nsp.oid from pg_namespace nsp where nsp.nspname = 'pg_catalog' )
                 and
                 -- ignore unused triggers
                   (pg_proc.prorettype <> ( select typ.oid from pg_type typ where typ.typname = 'trigger' ) or
                    pg_trigger.tgfoid is not null)
               offset 0
        ) ss
        order by (pcf).functionid::regprocedure::text, (pcf).lineno;
        "#,&[]
    )?;

    let rows = result.iter().collect::<Vec<_>>();

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for row in rows.iter() {
        let level = row
            .get::<_, Option<String>>("level")
            .unwrap_or_default()
            .to_string();
        if level == "error" {
            errors.push(row);
        } else if level == "warning" {
            warnings.push(row);
        }
    }

    if !errors.is_empty() {
        println!("plpgsql_check-cli: Found {} errors:", errors.len());
        for error in &errors {
            let error = Error {
                stack: "".to_string(),
                message: error
                    .get::<_, Option<String>>("message")
                    .unwrap_or_default()
                    .to_string(),
                severity: error
                    .get::<_, Option<String>>("level")
                    .unwrap_or_default()
                    .to_string(),
                error_code: error
                    .get::<_, Option<String>>("sqlstate")
                    .unwrap_or_default()
                    .to_string(),
                detail: error
                    .get::<_, Option<String>>("detail")
                    .unwrap_or_default()
                    .to_string(),
                hint: error
                    .get::<_, Option<String>>("hint")
                    .unwrap_or_default()
                    .to_string(),
                error_message_with_snippet: over_write_error_message_with_snippet(
                    ErrorSnippetInput {
                        block_code: error
                            .get::<_, Option<String>>("query")
                            .unwrap_or_default()
                            .to_string(),
                        function_name: error
                            .get::<_, Option<String>>("functionid")
                            .unwrap_or_default()
                            .to_string(),
                        error_message: error
                            .get::<_, Option<String>>("message")
                            .unwrap_or_default()
                            .to_string(),
                        error_code: error
                            .get::<_, Option<String>>("sqlstate")
                            .unwrap_or_default()
                            .to_string(),
                        position: error.get::<_, Option<i32>>("position").unwrap_or_default(),
                    },
                ),
            };

            log_database_error(error);
        }
    }

    if !warnings.is_empty() {
        println!("plpgsql_check-cli: Found {} warnings:", warnings.len());
        for warning in warnings {
            println!(
                "  - {}",
                warning
                    .get::<_, Option<String>>("message")
                    .unwrap_or_default()
            );
        }
    }

    client.close();

    if !errors.is_empty() {
        process::exit(1);
    }

    Ok(())
}
