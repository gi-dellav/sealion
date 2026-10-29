function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_conditional(component) {
    return is_tagged_list(component, "conditional_expression") ||
           is_tagged_list(component, "conditional_statement");
}
function conditional_predicate(component) {
   return list_ref(component, 1);
}
function conditional_consequent(component) {
   return list_ref(component, 2);
}
function conditional_alternative(component) {
   return list_ref(component, 3);
}

const my_cond_expr = 
    parse("true ? 1 : 2;");
is_conditional(my_cond_expr);
conditional_predicate(my_cond_expr);
conditional_consequent(my_cond_expr);
conditional_alternative(my_cond_expr);

// expected: [ 'literal', [ 2, null ] ]
