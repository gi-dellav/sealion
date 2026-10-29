function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_sequence(stmt) {
   return is_tagged_list(stmt, "sequence");
}
function sequence_statements(stmt) {   
   return head(tail(stmt));
}
function first_statement(stmts) {
   return head(stmts);
}
function rest_statements(stmts) {
   return tail(stmts);
}
function is_empty_sequence(stmts) {
   return is_null(stmts);
}
function is_last_statement(stmts) {
   return is_null(tail(stmts));
}

const my_sequence = parse("1; true; 45;");
is_sequence(my_sequence);
const my_actions = sequence_statements(my_sequence);
is_empty_sequence(my_actions);
is_last_statement(my_actions);
first_statement(my_actions);
rest_statements(my_actions);
list_ref(rest_statements(my_actions), 1);

// expected: [ 'literal', [ 45, null ] ]
