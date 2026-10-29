function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_declaration(component) {
    return is_tagged_list(component, "constant_declaration") ||
           is_tagged_list(component, "variable_declaration") ||
           is_tagged_list(component, "function_declaration");
}

const my_declaration_statement = parse("let x = 1;");
display(is_declaration(my_declaration_statement));
display(declaration_symbol(my_declaration_statement));
display(declaration_value_expression(my_declaration_statement));
