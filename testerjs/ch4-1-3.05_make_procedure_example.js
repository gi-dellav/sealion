function enclosing_environment(env) { return tail(env); }

function first_frame(env) { return head(env); }

const the_empty_environment = null;
const my_function = 
    make_function(
        list("x", "y"),
        list("return_statement", parse("x + y;")),
        the_empty_environment);
is_compound_function(my_function);
function_body(my_function);
function_environment(my_function);
function_parameters(my_function);
