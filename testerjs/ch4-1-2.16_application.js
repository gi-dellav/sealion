function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_application(component) {
   return is_tagged_list(component, "application");
}
function function_expression(component) {
   return head(tail(component));
}
function arg_expressions(component) {
   return head(tail(tail(component)));
}

const my_application = parse("math_pow(3, 4);");
display(is_application(my_application));
display(function_expression(my_application));
const my_expressions = arg_expressions(my_application);
display(no_arg_expressions(my_expressions));
display(first_arg_expression(my_expressions));
display(rest_arg_expressions(my_expressions));
