function user_read(prompt_string) {
    return prompt(prompt_string);
}
function empty_arglist() { return null; }

function adjoin_arg(arg, arglist) {
    return append(arglist, list(arg));
}
function is_last_argument_expression(arg_expression) {
    return is_null(tail(arg_expression));
}
const eceval_operations = 
    list(
      // args
      list("arg_expressions"      , arg_expressions),
      list("function_expression"  , function_expression),
      list("is_null"
                                  , is_null),
      list("head"        , head),
      list("is_last_argument_expression", is_last_argument_expression),
      list("tail"        , tail),

      //arg
      list("empty_arglist"        , empty_arglist),
      list("adjoin_arg"           , adjoin_arg),

      // comp (sequence)
      list("first_statement"     , first_statement),
      list("rest_statements"     , rest_statements),
      list("is_last_statement"   , is_last_statement),
      list("is_empty_sequence"   , is_empty_sequence),
      list("sequence_statements" , sequence_statements),

      // eval functions from meta-circular evaluator
      list("is_literal"  , is_literal),
      list("literal_value"       , literal_value),
      list("is_name"             , is_name),
      list("symbol_of_name"      , symbol_of_name),
      list("is_assignment"       , is_assignment),
      list("assignment_symbol"   , assignment_symbol),
      list("assignment_value_expression"
                                 , assignment_value_expression),
      list("assign_symbol_value" , assign_symbol_value),
      list("is_declaration"      , is_declaration),
      list("declaration_symbol"  , declaration_symbol),
      list("declaration_value_expression"
                                 , declaration_value_expression),
      list("assign_symbol_value" , assign_symbol_value),
      list("is_lambda_expression", is_lambda_expression),
      list("lambda_parameter_symbols"
                                 , lambda_parameter_symbols),
      list("lambda_body"         , lambda_body),
      list("is_return_statement" , is_return_statement),
      list("return_expression"   , return_expression),
      list("is_conditional"
                                 , is_conditional),
      list("conditional_predicate"
				 , conditional_predicate),
      list("conditional_consequent"
                                 , conditional_consequent),
      list("conditional_alternative"
                                 , conditional_alternative),
      list("is_sequence"         , is_sequence),
      list("is_block"            , is_block),
      list("block_body"          , block_body),
      list("scan_out_declarations"
                                 , scan_out_declarations),
      list("list_of_unassigned"  , list_of_unassigned),
      list("is_application"      , is_application),
      list("is_primitive_function"
                                 , is_primitive_function),
      list("apply_primitive_function"
                                 , apply_primitive_function),
      list("is_compound_function", is_compound_function),
      list("function_parameters" , function_parameters),
      list("function_environment", function_environment),
      list("function_body"       , function_body),
      list("extend_environment"  , extend_environment),
      list("make_function"       , make_function),

      list("lookup_symbol_value" , lookup_symbol_value),

      list("get_current_environment"
                                 , get_current_environment),
      list("set_current_environment"
                                 , set_current_environment),

      // Unsorted
      list("is_function_declaration"  , is_function_declaration),
      list("function_declaration_body"  , function_declaration_body),
      list("function_declaration_parameters"  , function_declaration_parameters),
      list("function_declaration_name"  , function_declaration_name),
      list("function_decl_to_constant_decl", function_decl_to_constant_decl),
      list("declaration_symbol"  , declaration_symbol),
      list("is_operator_combination", is_operator_combination),
      list("operator_combination_to_application", operator_combination_to_application),
      list("parse", parse),

      // generic helpers
      list("is_truthy", is_truthy),
      list("is_falsy", v => !is_truthy(v)),
      list("is_null", is_null),

      list("user_read", user_read),
      list("user_print", user_print),
      list("display", display),
      list("list", list)
    );
