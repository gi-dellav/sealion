function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function enclosing_environment(env) { return tail(env); }

function first_frame(env) { return head(env); }

const the_empty_environment = null;
function make_function(parameters, body, env) {
    return list("compound_function", parameters, body, env);
}
function is_compound_function(f) {
    return is_tagged_list(f, "compound_function");
}
function function_parameters(f) { return list_ref(f, 1); }

function function_body(f) { return list_ref(f, 2); }

function function_environment(f) { return list_ref(f, 3); }

const my_function = 
    make_function(
        list("x", "y"),
        list("return_statement", parse("x + y;")),
        the_empty_environment);
is_compound_function(my_function);
function_body(my_function);
function_environment(my_function);
function_parameters(my_function);

// expected: [ 'x', [ 'y', null ] ]
