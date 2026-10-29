const my_declaration_statement = parse("let x = 1;");
display(is_declaration(my_declaration_statement));
display(declaration_symbol(my_declaration_statement));
display(declaration_value_expression(my_declaration_statement));
