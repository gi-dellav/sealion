function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function enclosing_environment(env) { return tail(env); }

function first_frame(env) { return head(env); }

const the_empty_environment = null;
function make_return_value(content) {
    return list("return_value", content);
}
function is_return_value(value) {
    return is_tagged_list(value, "return_value");
}
function return_value_content(value) {
    return head(tail(value));
}

const my_return_value = make_return_value(42);
is_return_value(my_return_value);
return_value_content(my_return_value);

// expected: 42
