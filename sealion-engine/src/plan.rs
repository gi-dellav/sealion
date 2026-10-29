use std::collections::HashMap;

use boa_ast::{
    Declaration, Script, Statement, StatementList, StatementListItem,
    declaration::{Binding, LexicalDeclaration, VariableList},
    function::{FormalParameterList, FunctionDeclaration},
};
use boa_interner::Sym;

use crate::Error;

#[derive(Debug, Clone)]
pub struct FunctionPlan {
    pub id: usize,
    pub name: String,
    pub parent: Option<usize>,
    pub params: Vec<Sym>,
    pub body: StatementList,
    pub bindings: HashMap<Sym, usize>,
    pub children: Vec<usize>,
    pub is_script: bool,
    pub binding_name: Option<Sym>,
}

impl FunctionPlan {
    pub fn slot_count(&self) -> usize {
        self.bindings.len()
    }
}

pub fn build_program_plan(script: &Script) -> Result<Vec<FunctionPlan>, Error> {
    let mut plans = Vec::new();
    build_function_plan(
        &mut plans,
        None,
        "__script".to_string(),
        Vec::new(),
        script.statements().clone(),
        true,
    )?;
    Ok(plans)
}

fn build_function_plan(
    plans: &mut Vec<FunctionPlan>,
    parent: Option<usize>,
    name: String,
    params: Vec<Sym>,
    body: StatementList,
    is_script: bool,
) -> Result<usize, Error> {
    let id = plans.len();
    plans.push(FunctionPlan {
        id,
        name,
        parent,
        params: params.clone(),
        body: body.clone(),
        bindings: HashMap::new(),
        children: Vec::new(),
        is_script,
        binding_name: None,
    });

    for param in params {
        add_binding(plans, id, param);
    }

    scan_statements(plans, id, &body)?;
    Ok(id)
}

fn add_binding(plans: &mut [FunctionPlan], plan_id: usize, sym: Sym) {
    if !plans[plan_id].bindings.contains_key(&sym) {
        let slot = plans[plan_id].bindings.len();
        plans[plan_id].bindings.insert(sym, slot);
    }
}

fn scan_statements(
    plans: &mut Vec<FunctionPlan>,
    plan_id: usize,
    list: &StatementList,
) -> Result<(), Error> {
    for item in list.statements() {
        match item {
            StatementListItem::Statement(stmt) => scan_statement(plans, plan_id, stmt)?,
            StatementListItem::Declaration(decl) => scan_declaration(plans, plan_id, decl)?,
        }
    }
    Ok(())
}

fn scan_statement(
    plans: &mut Vec<FunctionPlan>,
    plan_id: usize,
    stmt: &Statement,
) -> Result<(), Error> {
    match stmt {
        Statement::Block(block) => scan_statements(plans, plan_id, block.statement_list())?,
        Statement::Var(var) => add_variable_list(plans, plan_id, &var.0)?,
        Statement::If(if_stmt) => {
            scan_statement(plans, plan_id, if_stmt.body())?;
            if let Some(else_node) = if_stmt.else_node() {
                scan_statement(plans, plan_id, else_node)?;
            }
        }
        Statement::WhileLoop(while_stmt) => {
            scan_statement(plans, plan_id, while_stmt.body())?;
        }
        Statement::Empty | Statement::Expression(_) | Statement::Return(_) => {}
        other => {
            return Err(Error::Unsupported(format!(
                "statement not supported in 0.1.0: {other:?}"
            )));
        }
    }
    Ok(())
}

fn scan_declaration(
    plans: &mut Vec<FunctionPlan>,
    plan_id: usize,
    decl: &Declaration,
) -> Result<(), Error> {
    match decl {
        Declaration::FunctionDeclaration(func) => {
            add_function_declaration(plans, plan_id, func)?;
        }
        Declaration::Lexical(lex) => {
            add_lexical_declaration(plans, plan_id, lex)?;
        }
        other => {
            return Err(Error::Unsupported(format!(
                "declaration not supported in 0.1.0: {other:?}"
            )));
        }
    }
    Ok(())
}

fn add_function_declaration(
    plans: &mut Vec<FunctionPlan>,
    plan_id: usize,
    func: &FunctionDeclaration,
) -> Result<(), Error> {
    add_binding(plans, plan_id, func.name().sym());

    let params = collect_params(func.parameters())?;
    let child_id = plans.len();
    let child_name = format!("js_fn_{child_id}");
    let child_body = func.body().statement_list().clone();

    let actual_id =
        build_function_plan(plans, Some(plan_id), child_name, params, child_body, false)?;
    plans[actual_id].binding_name = Some(func.name().sym());
    plans[plan_id].children.push(actual_id);
    Ok(())
}

fn add_lexical_declaration(
    plans: &mut [FunctionPlan],
    plan_id: usize,
    lex: &LexicalDeclaration,
) -> Result<(), Error> {
    add_variable_list(plans, plan_id, lex.variable_list())
}

fn add_variable_list(
    plans: &mut [FunctionPlan],
    plan_id: usize,
    list: &VariableList,
) -> Result<(), Error> {
    for var in list.as_ref() {
        match var.binding() {
            Binding::Identifier(id) => add_binding(plans, plan_id, id.sym()),
            Binding::Pattern(_) => {
                return Err(Error::Unsupported(
                    "destructuring bindings are not supported".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn collect_params(params: &FormalParameterList) -> Result<Vec<Sym>, Error> {
    let mut out = Vec::new();
    for param in params.as_ref() {
        if param.is_rest_param() {
            return Err(Error::Unsupported(
                "rest parameters are not supported".to_string(),
            ));
        }
        match param.variable().binding() {
            Binding::Identifier(id) => out.push(id.sym()),
            Binding::Pattern(_) => {
                return Err(Error::Unsupported(
                    "destructuring parameters are not supported".to_string(),
                ));
            }
        }
    }
    Ok(out)
}

pub fn resolve(plans: &[FunctionPlan], start: usize, sym: Sym) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut current = Some(start);
    while let Some(plan_id) = current {
        let plan = &plans[plan_id];
        if let Some(&slot) = plan.bindings.get(&sym) {
            return Some((depth, slot));
        }
        current = plan.parent;
        depth += 1;
    }
    None
}
