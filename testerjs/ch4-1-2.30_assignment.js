function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_assignment(component) {
    return is_tagged_list(component, "assignment");
}
function assignment_symbol(component) {
    return head(tail(head(tail(component))));
}
function assignment_value_expression(component) {
    return head(tail(tail(component)));
}

const my_assignment_statement = parse("x = 1;");
assignment_symbol(my_assignment_statement);
assignment_value_expression(my_assignment_statement);

// expected: [ 'literal', [ 1, null ] ]
