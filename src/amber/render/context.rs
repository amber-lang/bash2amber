use std::collections::{HashMap, HashSet};

use heraclitus_compiler::prelude::Message;

use super::syntax::{is_reserved_keyword, normalize_identifier};

#[derive(Debug, Clone)]
pub(super) struct TypeCommentParam {
    pub(super) name: String,
    pub(super) type_name: String,
}

#[derive(Debug, Clone)]
pub(super) struct TypeCommentSignature {
    pub(super) params: Vec<TypeCommentParam>,
    pub(super) return_contract: TypeCommentReturnContract,
    pub(super) comment_line: usize,
}

#[derive(Debug, Clone)]
pub(super) enum TypeCommentReturnContract {
    Null,
    TypedVariable {
        type_name: String,
        variable_name: String,
    },
}

#[derive(Debug, Clone)]
pub(super) enum FunctionTypeHint {
    Missing,
    Invalid { comment_line: usize, raw: String },
    Typed(TypeCommentSignature),
}

#[derive(Debug, Clone)]
pub(super) struct FunctionHint {
    pub(super) function_line: usize,
    pub(super) type_hint: FunctionTypeHint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FunctionRenderMode {
    Native,
    FallbackLiteral,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSig {
    pub(super) amber_name: String,
    pub(super) arity: usize,
    pub(super) returns_value: bool,
    pub(super) render_mode: FunctionRenderMode,
    pub(super) typed_signature: Option<TypeCommentSignature>,
}

#[derive(Debug, Clone)]
pub(super) struct RenderContext {
    var_aliases: HashMap<String, String>,
    declared_vars: HashSet<String>,
    functions: HashMap<String, FunctionSig>,
    positional_scopes: Vec<HashMap<usize, String>>,
    function_hints: HashMap<String, Vec<FunctionHint>>,
    function_hint_cursors: HashMap<String, usize>,
    return_var_stack: Vec<String>,
    source_path: Option<String>,
}

impl RenderContext {
    pub(super) fn new(
        function_hints: HashMap<String, Vec<FunctionHint>>,
        source_path: Option<String>,
    ) -> Self {
        Self {
            var_aliases: HashMap::new(),
            declared_vars: HashSet::new(),
            functions: HashMap::new(),
            positional_scopes: Vec::new(),
            function_hints,
            function_hint_cursors: HashMap::new(),
            return_var_stack: Vec::new(),
            source_path,
        }
    }

    pub(super) fn declare_var(&mut self, raw: &str) -> String {
        if let Some(existing) = self.var_aliases.get(raw) {
            self.declared_vars.insert(existing.clone());
            return existing.clone();
        }

        let candidate = self.next_unique_var_name(raw);
        self.var_aliases.insert(raw.to_string(), candidate.clone());
        self.declared_vars.insert(candidate.clone());
        candidate
    }

    pub(super) fn declare_local_var(&mut self, raw: &str) -> String {
        let candidate = self.next_unique_var_name(raw);
        self.var_aliases.insert(raw.to_string(), candidate.clone());
        self.declared_vars.insert(candidate.clone());
        candidate
    }

    fn next_unique_var_name(&self, raw: &str) -> String {
        let mut candidate = normalize_identifier(raw);
        if is_reserved_keyword(&candidate) {
            candidate.push_str("_var");
        }

        let base = candidate.clone();
        let mut index = 2usize;
        while self.declared_vars.contains(&candidate)
            || self.var_aliases.values().any(|name| name == &candidate)
        {
            candidate = format!("{base}_{index}");
            index += 1;
        }
        candidate
    }

    pub(super) fn resolve_var(&self, raw: &str) -> Option<String> {
        let alias = self.var_aliases.get(raw)?;
        self.declared_vars
            .contains(alias)
            .then(|| alias.to_string())
    }

    pub(super) fn bind_var_alias(&mut self, raw: &str, alias: &str) {
        self.var_aliases.insert(raw.to_string(), alias.to_string());
        self.declared_vars.insert(alias.to_string());
    }

    pub(super) fn with_child_scope(&self) -> Self {
        Self {
            var_aliases: self.var_aliases.clone(),
            declared_vars: self.declared_vars.clone(),
            functions: self.functions.clone(),
            positional_scopes: self.positional_scopes.clone(),
            function_hints: self.function_hints.clone(),
            function_hint_cursors: self.function_hint_cursors.clone(),
            return_var_stack: self.return_var_stack.clone(),
            source_path: self.source_path.clone(),
        }
    }

    pub(super) fn merge_from_child(&mut self, child: Self) {
        self.var_aliases.extend(child.var_aliases);
        self.declared_vars.extend(child.declared_vars);
        self.functions.extend(child.functions);

        for (name, child_cursor) in child.function_hint_cursors {
            let cursor = self.function_hint_cursors.entry(name).or_insert(0);
            *cursor = (*cursor).max(child_cursor);
        }
    }

    pub(super) fn register_function(&mut self, raw: &str, sig: FunctionSig) {
        self.functions.insert(raw.to_string(), sig);
    }

    pub(super) fn resolve_function(&self, raw: &str) -> Option<&FunctionSig> {
        self.functions.get(raw)
    }

    pub(super) fn next_function_hint(&mut self, raw: &str) -> Option<FunctionHint> {
        let hints = self.function_hints.get(raw)?;
        let cursor = self
            .function_hint_cursors
            .entry(raw.to_string())
            .or_insert(0);
        let hint = hints.get(*cursor).cloned();
        *cursor += 1;
        hint
    }

    pub(super) fn warn(&self, message: &str, comment: Option<&str>) {
        let rendered = if let Some(path) = &self.source_path {
            format!("{message} ({path})")
        } else {
            message.to_string()
        };
        let mut warn = Message::new_warn_msg(rendered);
        if let Some(comment) = comment {
            warn = warn.comment(comment);
        }
        warn.show();
        eprintln!();
    }

    pub(super) fn push_positional_scope_with_names(&mut self, names: &[String]) {
        let mut map = HashMap::new();
        for (index, name) in names.iter().enumerate() {
            map.insert(index + 1, name.clone());
        }
        self.positional_scopes.push(map);
    }

    pub(super) fn pop_positional_scope(&mut self) {
        self.positional_scopes.pop();
    }

    pub(super) fn push_return_var(&mut self, name: &str) {
        self.return_var_stack.push(name.to_string());
    }

    pub(super) fn pop_return_var(&mut self) {
        self.return_var_stack.pop();
    }

    pub(super) fn current_return_var(&self) -> Option<&str> {
        self.return_var_stack.last().map(String::as_str)
    }

    pub(super) fn resolve_positional(&self, index: usize) -> Option<String> {
        self.positional_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&index).cloned())
    }
}
