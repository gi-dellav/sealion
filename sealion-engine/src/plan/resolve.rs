use super::{Program, ScopeId};

pub fn is_const(program: &Program<'_>, start: ScopeId, name: &str) -> bool {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if scope.bindings.contains_key(name) {
            return scope.consts.contains(name);
        }
        current = scope.parent;
    }
    false
}

/// True when `name` resolves to a binding that starts in the temporal dead zone
/// and is not proven initialized before every load and store.
pub fn needs_tdz_check(program: &Program<'_>, start: ScopeId, name: &str) -> bool {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return scope.tdz.contains(&slot) && !program.no_tdz.contains(&(id, slot));
        }
        current = scope.parent;
    }
    false
}

/// Resolves `name` from `start`, returning the scope that owns the binding and
/// its slot.
pub fn resolve_binding(
    program: &Program<'_>,
    start: ScopeId,
    name: &str,
) -> Option<(ScopeId, usize)> {
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return Some((id, slot));
        }
        current = scope.parent;
    }
    None
}

/// Resolves `name` from `start`, returning how many environment pointers to
/// follow and the slot inside that environment.
pub fn resolve(program: &Program<'_>, start: ScopeId, name: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut current = Some(start);
    while let Some(id) = current {
        let scope = &program.scopes[id.0];
        if let Some(&slot) = scope.bindings.get(name) {
            return Some((depth, slot));
        }
        if scope.has_env() {
            depth += 1;
        }
        current = scope.parent;
    }
    None
}
