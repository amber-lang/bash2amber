use super::ast::*;
use super::rules;
use heraclitus_compiler::prelude::*;

pub fn parse(source: &str, path: Option<String>) -> Result<Program, String> {
    let mut compiler = Compiler::new("Bash", rules::get_rules());
    compiler.set_separator(SeparatorMode::Automatic("\\".to_string()));
    if let Some(path) = path.clone() {
        compiler.set_path(path);
    }
    compiler.load(source);

    let tokens = compiler
        .tokenize()
        .map_err(|(kind, pos)| lexer_error_to_string(kind, pos, source))?;

    let mut module = ProgramModule::new();
    let mut meta = DefaultMetadata::new(tokens, path, Some(source.to_string()));

    if let Err(failure) = module.parse(&mut meta) {
        return Err(failure_to_string(failure, source));
    }

    consume_separators(&mut meta);
    if let Some(tok) = meta.get_current_token() {
        return Err(format!(
            "Unexpected token '{}' at {}:{}",
            tok.word, tok.pos.0, tok.pos.1
        ));
    }

    Ok(module.program)
}

#[derive(Debug, Clone)]
struct ProgramModule {
    program: Program,
}

impl SyntaxModule<DefaultMetadata> for ProgramModule {
    syntax_name!("Program");

    fn new() -> Self {
        Self {
            program: Program {
                statements: Vec::new(),
            },
        }
    }

    fn parse(&mut self, meta: &mut DefaultMetadata) -> SyntaxResult {
        self.program.statements = parse_program(meta, &[])?;
        Ok(())
    }
}

fn parse_program(meta: &mut DefaultMetadata, stop_words: &[&str]) -> Result<Vec<Command>, Failure> {
    let mut statements = Vec::new();
    loop {
        consume_separators(meta);

        let Some(word) = current_word(meta) else {
            break;
        };

        if stop_words.contains(&word.as_str()) {
            break;
        }

        let statement = parse_statement(meta)?;
        statements.push(statement);
    }
    Ok(statements)
}

fn parse_statement(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let mut node = parse_and_or(meta)?;

    if current_word(meta).as_deref() == Some("&") {
        meta.increment_index();
        consume_connector_separators(meta);
        node = Command::Background(Box::new(node));
    }

    Ok(node)
}

fn parse_and_or(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let mut node = parse_pipeline(meta)?;

    loop {
        let Some(word) = current_word(meta) else {
            break;
        };

        let connector = match word.as_str() {
            "&&" => Connector::And,
            "||" => Connector::Or,
            _ => break,
        };

        meta.increment_index();
        consume_connector_separators(meta);

        let right = parse_pipeline(meta)?;
        node = Command::Connection(Connection {
            left: Box::new(node),
            op: connector,
            right: Box::new(right),
        });
    }

    Ok(node)
}

fn parse_pipeline(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let mut node = parse_command_unit(meta)?;

    loop {
        let Some(word) = current_word(meta) else {
            break;
        };

        if word != "|" {
            break;
        }

        meta.increment_index();
        consume_connector_separators(meta);

        let right = parse_command_unit(meta)?;
        node = Command::Connection(Connection {
            left: Box::new(node),
            op: Connector::Pipe,
            right: Box::new(right),
        });
    }

    Ok(node)
}

fn parse_command_unit(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let Some(word) = current_word(meta) else {
        return error!(meta, None, "Expected command");
    };

    match word.as_str() {
        "if" => parse_if(meta),
        "while" => parse_while(meta),
        "for" => parse_for(meta),
        "case" => parse_case(meta),
        "function" => parse_function_keyword(meta),
        "{" => parse_group(meta),
        _ => {
            if looks_like_arithmetic_command_start(&word) {
                parse_arithmetic(meta)
            } else if looks_like_function(meta) {
                parse_function_style(meta)
            } else {
                parse_simple(meta)
            }
        }
    }
}

fn looks_like_arithmetic_command_start(word: &str) -> bool {
    word == "((" || word.starts_with("((")
}

fn parse_arithmetic(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let Some(first) = consume_word(meta) else {
        return error!(meta, None, "Expected arithmetic command");
    };

    let first_chunk = if first == "((" {
        ""
    } else if let Some(rest) = first.strip_prefix("((") {
        rest
    } else {
        return error!(meta, None, "Expected '((' in arithmetic command");
    };

    let mut expression = String::new();
    let mut nested_parens = 0usize;
    let mut closed = false;

    consume_arithmetic_chunk(
        meta,
        first_chunk,
        &mut expression,
        &mut nested_parens,
        &mut closed,
    )?;

    while !closed {
        let Some(word) = consume_word(meta) else {
            return error!(meta, None, "Unterminated arithmetic command");
        };

        consume_arithmetic_chunk(
            meta,
            &word,
            &mut expression,
            &mut nested_parens,
            &mut closed,
        )?;
    }

    if !closed {
        return error!(meta, None, "Unterminated arithmetic command");
    }

    Ok(Command::Arithmetic(ArithmeticCommand {
        expression: expression.trim().to_string(),
    }))
}

fn consume_arithmetic_chunk(
    meta: &mut DefaultMetadata,
    chunk: &str,
    expression: &mut String,
    nested_parens: &mut usize,
    closed: &mut bool,
) -> Result<(), Failure> {
    if chunk.is_empty() {
        return Ok(());
    }

    let chars: Vec<char> = chunk.chars().collect();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut current = String::new();

    while i < chars.len() {
        let ch = chars[i];

        if escaped {
            current.push(ch);
            escaped = false;
            i += 1;
            continue;
        }

        if ch == '\\' {
            current.push(ch);
            escaped = true;
            i += 1;
            continue;
        }

        if ch == '\'' && !in_double {
            in_single = !in_single;
            current.push(ch);
            i += 1;
            continue;
        }

        if ch == '"' && !in_single {
            in_double = !in_double;
            current.push(ch);
            i += 1;
            continue;
        }

        if !in_single && !in_double {
            if ch == '(' {
                *nested_parens += 1;
                current.push(ch);
                i += 1;
                continue;
            }

            if ch == ')' {
                if i + 1 < chars.len() && chars[i + 1] == ')' && *nested_parens == 0 {
                    append_arithmetic_segment(expression, current.trim());
                    *closed = true;

                    let trailing: String = chars[i + 2..].iter().collect();
                    if !trailing.trim().is_empty() {
                        return error!(meta, None, "Unexpected token after arithmetic close");
                    }
                    return Ok(());
                }

                *nested_parens = nested_parens.saturating_sub(1);
                current.push(ch);
                i += 1;
                continue;
            }
        }

        current.push(ch);
        i += 1;
    }

    append_arithmetic_segment(expression, current.trim());
    Ok(())
}

fn append_arithmetic_segment(expression: &mut String, segment: &str) {
    if segment.is_empty() {
        return;
    }

    if !expression.is_empty() {
        expression.push(' ');
    }
    expression.push_str(segment);
}

fn parse_case(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    expect_word(meta, "case")?;
    consume_separators(meta);

    let Some(word) = consume_word(meta) else {
        return error!(meta, None, "Expected case subject word");
    };

    consume_separators(meta);
    expect_word(meta, "in")?;

    let mut clauses = Vec::new();
    loop {
        consume_case_statement_separators(meta);

        if current_word(meta).as_deref() == Some("esac") {
            meta.increment_index();
            break;
        }

        let clause = parse_case_clause(meta)?;
        let is_end = clause.terminator == CaseClauseTerminator::End;
        clauses.push(clause);
        if is_end {
            break;
        }
    }

    Ok(Command::Case(CaseCommand { word, clauses }))
}

fn parse_case_clause(meta: &mut DefaultMetadata) -> Result<CaseClause, Failure> {
    let patterns = parse_case_patterns(meta)?;
    let (body, terminator) = parse_case_clause_body(meta)?;
    Ok(CaseClause {
        patterns,
        body,
        terminator,
    })
}

fn parse_case_patterns(meta: &mut DefaultMetadata) -> Result<Vec<String>, Failure> {
    consume_case_statement_separators(meta);

    let mut patterns = Vec::new();
    let mut current = String::new();
    let mut consumed_any = false;

    loop {
        let Some(mut token) = consume_word(meta) else {
            return error!(meta, None, "Unterminated case pattern list");
        };

        if !consumed_any {
            if token == "(" {
                consumed_any = true;
                continue;
            }
            if let Some(stripped) = token.strip_prefix('(') {
                token = stripped.to_string();
            }
        }

        consumed_any = true;

        let mut reached_end = false;
        if token == ")" {
            token.clear();
            reached_end = true;
        } else if let Some(stripped) = token.strip_suffix(')') {
            token = stripped.to_string();
            reached_end = true;
        }

        if token == "|" {
            if current.trim().is_empty() {
                return error!(meta, None, "Invalid empty case pattern");
            }
            patterns.push(current.trim().to_string());
            current.clear();
        } else if !token.is_empty() {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&token);
        }

        if reached_end {
            if !current.trim().is_empty() {
                patterns.push(current.trim().to_string());
            }
            break;
        }
    }

    if patterns.is_empty() {
        return error!(meta, None, "Expected at least one case pattern");
    }

    Ok(patterns)
}

fn parse_case_clause_body(
    meta: &mut DefaultMetadata,
) -> Result<(Vec<Command>, CaseClauseTerminator), Failure> {
    let mut body = Vec::new();

    loop {
        consume_case_statement_separators(meta);

        if let Some(term) = consume_case_clause_terminator(meta) {
            return Ok((body, term));
        }

        if current_word(meta).as_deref() == Some("esac") {
            meta.increment_index();
            return Ok((body, CaseClauseTerminator::End));
        }

        let statement = parse_statement(meta)?;
        body.push(statement);
    }
}

fn consume_case_statement_separators(meta: &mut DefaultMetadata) {
    loop {
        let Some(word) = current_word(meta) else {
            return;
        };

        if word == "\n" {
            meta.increment_index();
            continue;
        }

        if is_comment(&word) {
            skip_comment(meta);
            continue;
        }

        if word == ";" {
            let next = peek_word(meta, 1);
            if next.as_deref() != Some(";") && next.as_deref() != Some("&") {
                meta.increment_index();
                continue;
            }
        }

        return;
    }
}

fn consume_case_clause_terminator(meta: &mut DefaultMetadata) -> Option<CaseClauseTerminator> {
    let current = current_word(meta)?;

    if current == ";;" {
        meta.increment_index();
        return Some(CaseClauseTerminator::Break);
    }
    if current == ";&" {
        meta.increment_index();
        return Some(CaseClauseTerminator::Fallthrough);
    }
    if current == ";;&" {
        meta.increment_index();
        return Some(CaseClauseTerminator::TestNext);
    }

    if current != ";" {
        return None;
    }

    match (peek_word(meta, 1), peek_word(meta, 2)) {
        (Some(next), Some(third)) if next == ";" && third == "&" => {
            meta.increment_index();
            meta.increment_index();
            meta.increment_index();
            Some(CaseClauseTerminator::TestNext)
        }
        (Some(next), _) if next == ";" => {
            meta.increment_index();
            meta.increment_index();
            Some(CaseClauseTerminator::Break)
        }
        (Some(next), _) if next == "&" => {
            meta.increment_index();
            meta.increment_index();
            Some(CaseClauseTerminator::Fallthrough)
        }
        _ => None,
    }
}

fn parse_if(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    expect_word(meta, "if")?;
    let command = parse_if_tail(meta)?;
    Ok(Command::If(command))
}

fn parse_if_tail(meta: &mut DefaultMetadata) -> Result<IfCommand, Failure> {
    consume_separators(meta);
    let condition = parse_statement(meta)?;
    consume_separators(meta);
    expect_word(meta, "then")?;

    let then_body = parse_program(meta, &["else", "elif", "fi"])?;

    let else_body = match current_word(meta).as_deref() {
        Some("else") => {
            meta.increment_index();
            let body = parse_program(meta, &["fi"])?;
            expect_word(meta, "fi")?;
            Some(body)
        }
        Some("elif") => {
            meta.increment_index();
            let nested = parse_if_tail(meta)?;
            Some(vec![Command::If(nested)])
        }
        _ => {
            expect_word(meta, "fi")?;
            None
        }
    };

    Ok(IfCommand {
        condition: Box::new(condition),
        then_body,
        else_body,
    })
}

fn parse_while(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    expect_word(meta, "while")?;
    consume_separators(meta);

    let condition = parse_statement(meta)?;
    consume_separators(meta);
    expect_word(meta, "do")?;

    let body = parse_program(meta, &["done"])?;
    expect_word(meta, "done")?;

    Ok(Command::While(WhileCommand {
        condition: Box::new(condition),
        body,
    }))
}

fn parse_for(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    expect_word(meta, "for")?;
    consume_separators(meta);

    if is_c_style_for_start(meta) {
        return parse_c_style_for(meta);
    }

    let Some(name) = consume_word(meta) else {
        return error!(meta, None, "Expected loop variable name");
    };

    if !is_identifier(&name) {
        return error!(meta, None, format!("Invalid loop variable name '{name}'"));
    }

    consume_separators(meta);
    expect_word(meta, "in")?;

    let mut items = Vec::new();
    loop {
        let Some(word) = current_word(meta) else {
            return error!(meta, None, "Unterminated for loop");
        };

        if is_separator(&word) || word == "do" {
            break;
        }

        items.push(word);
        meta.increment_index();
    }

    consume_separators(meta);
    expect_word(meta, "do")?;

    let body = parse_program(meta, &["done"])?;
    expect_word(meta, "done")?;

    Ok(Command::For(ForCommand {
        variable: name,
        items,
        body,
    }))
}

fn is_c_style_for_start(meta: &DefaultMetadata) -> bool {
    current_word(meta)
        .as_deref()
        .map(|word| word == "((" || word.starts_with("(("))
        .unwrap_or(false)
}

fn parse_c_style_for(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let mut init_tokens = Vec::new();
    let mut cond_tokens = Vec::new();
    let mut update_tokens = Vec::new();

    if let Some(seed) = consume_c_style_for_open(meta)? {
        init_tokens.push(seed);
    }
    collect_c_style_for_segment(meta, &mut init_tokens, CStyleSegmentDelimiter::Semicolon)?;
    collect_c_style_for_segment(meta, &mut cond_tokens, CStyleSegmentDelimiter::Semicolon)?;
    collect_c_style_for_segment(meta, &mut update_tokens, CStyleSegmentDelimiter::Close)?;

    consume_separators(meta);
    expect_word(meta, "do")?;
    let body = parse_program(meta, &["done"])?;
    expect_word(meta, "done")?;

    Ok(Command::CStyleFor(CStyleForCommand {
        init: normalize_c_style_segment(&init_tokens),
        condition: normalize_c_style_segment(&cond_tokens),
        update: normalize_c_style_segment(&update_tokens),
        body,
    }))
}

fn consume_c_style_for_open(meta: &mut DefaultMetadata) -> Result<Option<String>, Failure> {
    let Some(word) = consume_word(meta) else {
        return error!(meta, None, "Expected '((' in C-style for loop");
    };

    if word == "((" {
        return Ok(None);
    }

    if let Some(rest) = word.strip_prefix("((") {
        if rest.is_empty() {
            return Ok(None);
        }
        return Ok(Some(rest.to_string()));
    }

    error!(meta, None, "Expected '((' in C-style for loop")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CStyleSegmentDelimiter {
    Semicolon,
    Close,
}

fn collect_c_style_for_segment(
    meta: &mut DefaultMetadata,
    out: &mut Vec<String>,
    delimiter: CStyleSegmentDelimiter,
) -> Result<(), Failure> {
    loop {
        let Some(word) = current_word(meta) else {
            return error!(meta, None, "Unterminated C-style for loop");
        };

        match delimiter {
            CStyleSegmentDelimiter::Semicolon => {
                if word == ";" {
                    meta.increment_index();
                    return Ok(());
                }
            }
            CStyleSegmentDelimiter::Close => {
                if word == "))" {
                    meta.increment_index();
                    return Ok(());
                }
                if let Some(prefix) = word.strip_suffix("))") {
                    if !prefix.is_empty() {
                        out.push(prefix.to_string());
                    }
                    meta.increment_index();
                    return Ok(());
                }
            }
        }

        out.push(word);
        meta.increment_index();
    }
}

fn normalize_c_style_segment(tokens: &[String]) -> String {
    tokens.join(" ")
}

fn parse_function_keyword(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    expect_word(meta, "function")?;
    consume_separators(meta);

    let Some(name) = consume_word(meta) else {
        return error!(meta, None, "Expected function name");
    };

    if !is_identifier(&name) {
        return error!(meta, None, format!("Invalid function name '{name}'"));
    }

    if current_word(meta).as_deref() == Some("()") {
        meta.increment_index();
    }

    consume_separators(meta);
    let body = parse_block_in_braces(meta, "function body")?;

    Ok(Command::Function(FunctionDef { name, body }))
}

fn parse_function_style(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let Some(token) = consume_word(meta) else {
        return error!(meta, None, "Expected function name");
    };

    let name = if let Some(name) = token.strip_suffix("()") {
        name.to_string()
    } else if current_word(meta).as_deref() == Some("()") {
        meta.increment_index();
        token
    } else {
        return error!(meta, None, "Expected function declaration");
    };

    if !is_identifier(&name) {
        return error!(meta, None, format!("Invalid function name '{name}'"));
    }

    consume_separators(meta);
    let body = parse_block_in_braces(meta, "function body")?;

    Ok(Command::Function(FunctionDef { name, body }))
}

fn parse_group(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let body = parse_block_in_braces(meta, "group")?;
    Ok(Command::Group(body))
}

fn parse_block_in_braces(
    meta: &mut DefaultMetadata,
    block_name: &str,
) -> Result<Vec<Command>, Failure> {
    expect_word(meta, "{")?;
    let body = parse_program(meta, &["}"])?;
    if current_word(meta).as_deref() != Some("}") {
        return error!(meta, None, format!("Unterminated {block_name}"));
    }
    meta.increment_index();
    Ok(body)
}

fn parse_simple(meta: &mut DefaultMetadata) -> Result<Command, Failure> {
    let mut words = Vec::new();
    let mut command_substitution_depth = 0usize;

    loop {
        let Some(word) = current_word(meta) else {
            break;
        };

        if command_substitution_depth == 0 && (is_simple_stop(&word) || is_comment(&word)) {
            break;
        }

        command_substitution_depth =
            update_command_substitution_depth(command_substitution_depth, &word);
        words.push(word);
        meta.increment_index();
    }

    if words.is_empty() {
        return error!(meta, meta.get_current_token(), "Expected command");
    }

    Ok(Command::Simple(SimpleCommand {
        words: normalize_simple_words(words),
    }))
}

fn update_command_substitution_depth(mut depth: usize, word: &str) -> usize {
    let chars: Vec<char> = word.chars().collect();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while i < chars.len() {
        let ch = chars[i];

        if escaped {
            escaped = false;
            i += 1;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            i += 1;
            continue;
        }

        if ch == '\'' && !in_double {
            in_single = !in_single;
            i += 1;
            continue;
        }

        if ch == '"' && !in_single {
            in_double = !in_double;
            i += 1;
            continue;
        }

        if !in_single && !in_double && ch == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            let is_arithmetic = i + 2 < chars.len() && chars[i + 2] == '(';
            if !is_arithmetic {
                depth = depth.saturating_add(1);
                i += 2;
                continue;
            }
        }

        if !in_single && !in_double && ch == ')' && depth > 0 {
            depth -= 1;
        }

        i += 1;
    }

    depth
}

fn normalize_simple_words(words: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let current = &words[index];
        if looks_like_assignment_prefix(current) && index + 1 < words.len() {
            let next = &words[index + 1];
            if next.starts_with('\"') || next.starts_with('\'') || next.starts_with('`') {
                result.push(format!("{current}{next}"));
                index += 2;
                continue;
            }
        }
        result.push(current.clone());
        index += 1;
    }
    result
}

fn looks_like_assignment_prefix(word: &str) -> bool {
    if !word.ends_with('=') || word == "=" {
        return false;
    }
    let name = &word[..word.len() - 1];
    is_identifier(name)
}

fn skip_comment(meta: &mut DefaultMetadata) {
    while let Some(word) = current_word(meta) {
        meta.increment_index();
        if word == "\n" {
            break;
        }
    }
}

fn consume_separators(meta: &mut DefaultMetadata) -> usize {
    let mut consumed = 0;
    while let Some(word) = current_word(meta) {
        if is_separator(&word) {
            meta.increment_index();
            consumed += 1;
        } else if is_comment(&word) {
            skip_comment(meta);
            consumed += 1;
        } else {
            break;
        }
    }
    consumed
}

fn consume_connector_separators(meta: &mut DefaultMetadata) {
    while let Some(word) = current_word(meta) {
        if word == "\n" {
            meta.increment_index();
        } else if is_comment(&word) {
            skip_comment(meta);
        } else {
            break;
        }
    }
}

fn expect_word(meta: &mut DefaultMetadata, expected: &str) -> Result<(), Failure> {
    if token(meta, expected).is_ok() {
        return Ok(());
    }
    error!(
        meta,
        meta.get_current_token(),
        format!("Expected '{expected}'")
    )
}

fn current_word(meta: &DefaultMetadata) -> Option<String> {
    meta.get_current_token().map(|token| token.word)
}

fn consume_word(meta: &mut DefaultMetadata) -> Option<String> {
    let token = meta.get_current_token();
    if token.is_some() {
        meta.increment_index();
    }
    token.map(|tok| tok.word)
}

fn peek_word(meta: &DefaultMetadata, offset: usize) -> Option<String> {
    meta.get_token_at(meta.get_index() + offset)
        .map(|tok| tok.word)
}

fn looks_like_function(meta: &DefaultMetadata) -> bool {
    let Some(first) = current_word(meta) else {
        return false;
    };

    if let Some(name) = first.strip_suffix("()") {
        return is_identifier(name);
    }

    if !is_identifier(&first) {
        return false;
    }

    matches!(meta.get_token_at(meta.get_index() + 1), Some(tok) if tok.word == "()")
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_separator(word: &str) -> bool {
    matches!(word, ";" | "\n")
}

fn is_comment(word: &str) -> bool {
    word.starts_with('#')
}

fn is_simple_stop(word: &str) -> bool {
    is_separator(word) || matches!(word, "|" | "&&" | "||" | "&")
}

fn lexer_error_to_string(kind: LexerErrorType, pos: PositionInfo, source: &str) -> String {
    let (line, col) = pos.get_pos_by_code(source);
    let name = match kind {
        LexerErrorType::Singleline => "single-line region",
        LexerErrorType::Unclosed => "unclosed region",
    };

    format!("Lexer error ({name}) at {line}:{col}")
}

fn failure_to_string(failure: Failure, source: &str) -> String {
    match failure {
        Failure::Loud(message) => {
            let description = message
                .message
                .clone()
                .unwrap_or_else(|| "Parse error".to_string());

            if let Some(pos) = message.trace.first() {
                let (line, col) = pos.get_pos_by_code(source);
                if let Some(comment) = message.comment.clone() {
                    format!("{description} at {line}:{col} ({comment})")
                } else {
                    format!("{description} at {line}:{col}")
                }
            } else {
                description
            }
        }
        Failure::Quiet(pos) => {
            let (line, col) = pos.get_pos_by_code(source);
            format!("Parse error at {line}:{col}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_simple_words(command: &Command, expected: &[&str]) {
        match command {
            Command::Simple(simple) => {
                let expected_words = expected
                    .iter()
                    .map(|word| word.to_string())
                    .collect::<Vec<_>>();
                assert_eq!(simple.words, expected_words);
            }
            _ => panic!("expected simple command, got {command:?}"),
        }
    }

    #[test]
    fn parses_simple_command_with_keyword_as_argument() {
        let program = parse("echo done\n", None).expect("script should parse");
        assert_eq!(program.statements.len(), 1);
        assert_simple_words(&program.statements[0], &["echo", "done"]);
    }

    #[test]
    fn parses_background_statement_separator() {
        let program = parse("echo a & echo b\n", None).expect("script should parse");
        assert_eq!(program.statements.len(), 2);

        match &program.statements[0] {
            Command::Background(inner) => assert_simple_words(inner, &["echo", "a"]),
            _ => panic!(
                "expected background command, got {:?}",
                program.statements[0]
            ),
        }

        assert_simple_words(&program.statements[1], &["echo", "b"]);
    }

    #[test]
    fn gives_pipeline_higher_precedence_than_or() {
        let program = parse("a || b | c\n", None).expect("script should parse");
        assert_eq!(program.statements.len(), 1);

        let Command::Connection(or_connection) = &program.statements[0] else {
            panic!("expected OR connection, got {:?}", program.statements[0]);
        };
        assert_eq!(or_connection.op, Connector::Or);
        assert_simple_words(&or_connection.left, &["a"]);

        let Command::Connection(pipe_connection) = or_connection.right.as_ref() else {
            panic!(
                "expected pipeline on OR right side, got {:?}",
                or_connection.right
            );
        };
        assert_eq!(pipe_connection.op, Connector::Pipe);
        assert_simple_words(&pipe_connection.left, &["b"]);
        assert_simple_words(&pipe_connection.right, &["c"]);
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;
    use heraclitus_compiler::prelude::*;

    #[test]
    fn debug_dollar_hash_tokens() {
        let source = "while [[ $# -gt 0 ]]; do\n  echo hi\ndone\n";
        let mut compiler = Compiler::new("Bash", rules::get_rules());
        compiler.set_separator(SeparatorMode::Automatic("\\".to_string()));
        compiler.load(source);
        let tokens = compiler.tokenize().expect("tokenize");
        for (i, tok) in tokens.iter().enumerate() {
            eprintln!("Token[{i}]: word={:?} pos={:?}", tok.word, tok.pos);
        }
        // Also try parsing
        let result = parse(source, None);
        eprintln!("Parse result: {:?}", result);
    }
}
