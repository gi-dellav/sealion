const my_sequence = parse("1; true; 45;");
is_sequence(my_sequence);
const my_actions = sequence_statements(my_sequence);
is_empty_sequence(my_actions);
is_last_statement(my_actions);
first_statement(my_actions);
rest_statements(my_actions);
list_ref(rest_statements(my_actions), 1);
