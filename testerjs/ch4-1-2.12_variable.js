function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_name(component) {
    return is_tagged_list(component, "name");
}

const my_name_statement = parse("x;");
display(is_name(my_name_statement));
display(symbol_of_name(my_name_statement));
