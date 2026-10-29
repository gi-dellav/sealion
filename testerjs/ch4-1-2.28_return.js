function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_return_statement(component) {
   return is_tagged_list(component, "return_statement");
}
function return_expression(component) {
   return head(tail(component));
}

const my_function_declaration = parse("function f(x) { return x; }");
const my_return = list_ref(my_function_declaration, 3);
list_ref(my_return, 1);

// expected: [ 'name', [ 'x', null ] ]
