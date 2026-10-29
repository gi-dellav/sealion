function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_name(component) {
    return is_tagged_list(component, "name");
}
function symbol_of_name(component) {
    return head(tail(component));
}
function is_lambda_expression(component) {
    return is_tagged_list(component, "lambda_expression");
}
function lambda_parameter_symbols(component) {
    return map(symbol_of_name, head(tail(component)));
}
function lambda_body(component) {
    return head(tail(tail(component)));
}

const my_lambda = parse("x => x");
display(is_lambda_expression(my_lambda));
display(lambda_parameter_symbols(my_lambda));
display(lambda_body(my_lambda));
