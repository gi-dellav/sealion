function enclosing_environment(env) { return tail(env); }

function first_frame(env) { return head(env); }

const the_empty_environment = null;
const my_return_value = make_return_value(42);
is_return_value(my_return_value);
return_value_content(my_return_value);
