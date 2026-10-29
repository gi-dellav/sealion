let rule_counter = 0;

function new_rule_application_id() {
   rule_counter = rule_counter + 1;
   return rule_counter;
}
function make_new_variable(variable, rule_application_id) {
   return make_name(symbol_of_name(variable) + "_" +
                    stringify(rule_application_id));
}
