function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_literal(component) {
    return is_tagged_list(component, "literal");
}
function literal_value(component) {    
    return head(tail(component));
}

const my_program = parse("true; 1;");
const my_true_statement = list_ref(list_ref(my_program, 1), 0);
is_literal(my_true_statement);

// expected: true
