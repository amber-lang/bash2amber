use std::collections::{HashMap, HashSet};

use super::syntax::{is_reserved_keyword, normalize_identifier};

#[derive(Debug, Default, Clone)]
pub(super) struct RenderContext {
    var_aliases: HashMap<String, String>,
    declared_vars: HashSet<String>,
    functions: HashMap<String, FunctionSig>,
    positional_scopes: Vec<HashMap<usize, String>>,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSig {
    pub(super) amber_name: String,
    pub(super) arity: usize,
    pub(super) returns_value: bool,
}

impl RenderContext {
    pub(super) fn new() -> Self {
        Self {
            var_aliases: HashMap::new(),
            declared_vars: HashSet::new(),
            functions: HashMap::new(),
            positional_scopes: Vec::new(),
        }
    }

    pub(super) fn declare_var(&mut self, raw: &str) -> String {
        if let Some(existing) = self.var_aliases.get(raw) {
            self.declared_vars.insert(existing.clone());
            return existing.clone();
        }

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

        self.var_aliases.insert(raw.to_string(), candidate.clone());
        self.declared_vars.insert(candidate.clone());
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
        }
    }

    pub(super) fn merge_from_child(&mut self, child: Self) {
        self.var_aliases.extend(child.var_aliases);
        self.declared_vars.extend(child.declared_vars);
        self.functions.extend(child.functions);
    }

    pub(super) fn register_function(
        &mut self,
        raw: &str,
        amber_name: String,
        arity: usize,
        returns_value: bool,
    ) {
        self.functions.insert(
            raw.to_string(),
            FunctionSig {
                amber_name,
                arity,
                returns_value,
            },
        );
    }

    pub(super) fn resolve_function(&self, raw: &str) -> Option<&FunctionSig> {
        self.functions.get(raw)
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

    pub(super) fn resolve_positional(&self, index: usize) -> Option<String> {
        self.positional_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&index).cloned())
    }
}
