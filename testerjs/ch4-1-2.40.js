function make_conditional_expr_decl(predicate, consequent_expression, alternative_expression) {
    return list("conditional_expression", predicate, consequent_expression, alternative_expression);
}
function make_literal(value) {
    return list("literal", value);
}

// Syntax selectors
function logical_operation(component) {
    return head(tail(component));
}
function first_expression(component) {
    return head(tail(tail(component)));
}
function second_expression(component) {
    return head(tail(tail(tail(component))));
}

function logical_comp_decl_to_conditional_expr_decl(component) {
    const operation = logical_operation(component);
    
    return operation === "&&"
        ? make_conditional_expr_decl(
            first_expression(component),
            second_expression(component),
            false
        )
        : operation === "||"
        ? make_conditional_expr_decl(
            first_expression(component),
            true,
            second_expression(component)
        )
        : error(component, "unknown operation -- logical_comp_decl_to_conditional_expr_decl");
}

display(logical_comp_decl_to_conditional_expr_decl(parse("a && b;")));
display(logical_comp_decl_to_conditional_expr_decl(parse("a || b;")));
display(logical_comp_decl_to_conditional_expr_decl(parse("(a && !b) || (!a && b);")));
