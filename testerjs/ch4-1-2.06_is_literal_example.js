const my_program = parse("true; 1;");
const my_true_statement = list_ref(list_ref(my_program, 1), 0);
is_literal(my_true_statement);
