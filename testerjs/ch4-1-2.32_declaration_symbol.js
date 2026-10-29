function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function declaration_symbol(component) {
    return symbol_of_name(head(tail(component)));
}
function declaration_value_expression(component) {
    return head(tail(tail(component)));
}

const my_declaration_statement = parse("let x = 1;");
display(is_declaration(my_declaration_statement));
display(declaration_symbol(my_declaration_statement));
display(declaration_value_expression(my_declaration_statement));
