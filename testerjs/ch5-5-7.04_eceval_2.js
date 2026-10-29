function prompt_for_input(input_prompt) {
    const input = prompt(input_prompt);
    if (is_null(input)) {
        display("--- evaluator terminated ---");
        return null;
    } else {
        display(input_prompt + "\n" + input + "\n----------------------------");
        return parse(input);
    }
}
let current_environment = the_global_environment;

function get_current_environment() {
    return current_environment;
}

function set_current_environment(env) {
    current_environment = env;
}
const eceval_operations = 
    list(
      // args
      list("arg_expressions"      , arg_expressions),
      list("function_expression"  , function_expression),
      list("is_null"
                                  , is_null),
      list("head"        , head),
      list("is_last_argument_expression"
                                  , a => is_null(tail(a))),
      list("tail"        , tail),

      //arg
      list("empty_arglist"        , () => null),
      list("adjoin_arg"           , (val, argl) => append(argl,
                                                      list(val))),

      // comp (sequence)
      list("first_statement"     , first_statement),
      list("rest_statements"     , rest_statements),
      list("is_last_statement"   , is_last_statement),
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

      // generic helpers
      list("is_truthy", is_truthy),
      list("is_falsy"                , x => ! is_truthy(x)), 
      list("is_null", is_null),
      list("is_pair"                 , is_pair),
      list("is_number"               , is_number),
      list("append"                  , append),
      list("pair"                    , pair),

      list(
        "lookup_symbol_value"        , lookup_symbol_value),
      list("get_current_environment" , get_current_environment),
      list("set_current_environment" , set_current_environment),

      list("user_read"               , prompt),
      list("user_print"              , user_print),
      list("parse"                   , parse),
      list("display"                 , display),
      list("make_compiled_function"  , make_compiled_function),
      list("is_compiled_function"    , is_compiled_function),
      list("compiled_function_env"   , compiled_function_env),
      list("compiled_function_entry" , compiled_function_entry),
      list("list"                    , list)
      
    );

const eceval_controller =
list(
      branch(label("external_entry")), // branches if flag is set    

      "read_evaluate_print_loop",
      perform(list(op("initialize_stack"))),
      assign("comp", list(op("user_read"), constant("EC-evaluate input:"))),
      test(list(op("is_null"), reg("comp"))),
      branch(label("evaluator_done")),
      assign("comp", list(op("parse"), reg("comp"))),
      assign("env", list(op("get_current_environment"))),

      assign("val", list(op("scan_out_declarations"), reg("comp"))),
      save("comp"), // temporarily store to comp
      assign("comp", list(op("list_of_unassigned"), reg("val"))),
      assign("env", list(op("extend_environment"), 
                         reg("val"), reg("comp"), reg("env"))),
      perform(list(op("set_current_environment"), reg("env"))),
      restore("comp"),
      assign("continue", label("print_result")),
      go_to(label("eval_dispatch")),

      "external_entry",
      perform(list(op("initialize_stack"))),
      assign("env", list(op("get_current_environment"))),
      assign("continue", label("print_result")),
      go_to(reg("val")),

      "print_result",
      perform(list(op("user_print"),
                   constant("EC-evaluate value:"), reg("val"))),
      go_to(label("read_evaluate_print_loop")), 

      "eval_dispatch",
      test(list(op("is_literal"), reg("comp"))),
      branch(label("ev_literal")),
      test(list(op("is_name"), reg("comp"))),
      branch(label("ev_name")),
      test(list(op("is_operator_combination"), reg("comp"))),
      branch(label("ev_operator_combination")),
      test(list(op("is_function_declaration"), reg("comp"))),
      branch(label("ev_function_declaration")),
      test(list(op("is_operator_combination"), reg("comp"))),
      branch(label("ev_operator_combination")),
      test(list(op("is_declaration"), reg("comp"))),
      branch(label("ev_declaration")),
      test(list(op("is_assignment"), reg("comp"))),
      branch(label("ev_assignment")),
      test(list(op("is_return_statement"), reg("comp"))),
      branch(label("ev_return")),
      test(list(op("is_conditional"), reg("comp"))),
      branch(label("ev_conditional")),
      test(list(op("is_lambda_expression"), reg("comp"))),
      branch(label("ev_lambda")),
      test(list(op("is_sequence"), reg("comp"))),
      branch(label("ev_sequence_start")),
      test(list(op("is_block"), reg("comp"))),
      branch(label("ev_block")),
      test(list(op("is_application"), reg("comp"))),
      branch(label("ev_application")),
      go_to(label("unknown_component_type")),

      "ev_return",
      revert_stack_to_marker(),
      restore("continue"),
      assign("comp", list(op("return_expression"), reg("comp"))),
      go_to(label("eval_dispatch")),

      "ev_literal",
      assign("val", list(op("literal_value"), reg("comp"))),
      go_to(reg("continue")),
      
      "ev_name",
      assign("comp", list(op("symbol_of_name"), reg("comp"))),
      assign("val", list(op("lookup_symbol_value"), reg("comp"), reg("env"))),
      go_to(reg("continue")),
      
      "ev_lambda",
      assign("unev", list(op("lambda_parameter_symbols"), reg("comp"))),
      assign("comp", list(op("lambda_body"), reg("comp"))),
      assign("val", list(op("make_function"),
                         reg("unev"), reg("comp"), reg("env"))),
      go_to(reg("continue")),

      "ev_operator_combination",
      assign("comp", list(op("operator_combination_to_application"),
                          reg("comp"), reg("env"))),
      
      "ev_application",
      save("continue"),
      save("env"),
      assign("unev", list(op("arg_expressions"), reg("comp"))),
      save("unev"),
      assign("comp", list(op("function_expression"), reg("comp"))),
      assign("continue", label("ev_appl_did_function_expression")),
      go_to(label("eval_dispatch")),
      
      "ev_appl_did_function_expression",
      restore("unev"), // the args
      restore("env"),
      assign("argl", list(op("empty_arglist"))),
      assign("fun", reg("val")), // the function_expression
      test(list(op("is_null"),
                reg("unev"))),
      branch(label("apply_dispatch")),
      save("fun"),
      
      "ev_appl_argument_expression_loop",
      save("argl"),
      assign("comp", list(op("head"), reg("unev"))),
      test(list(op("is_last_argument_expression"),
                reg("unev"))),
      branch(label("ev_appl_last_arg")),
      save("env"),
      save("unev"),
      assign("continue", label("ev_appl_accumulate_arg")),
      go_to(label("eval_dispatch")),
      
      "ev_appl_accumulate_arg",
      restore("unev"),
      restore("env"),
      restore("argl"),
      assign("argl", list(op("adjoin_arg"),
                          reg("val"), reg("argl"))),
      assign("unev", list(op("tail"), reg("unev"))),
      go_to(label("ev_appl_argument_expression_loop")),
      
      "ev_appl_last_arg",
      assign("continue", label("ev_appl_accum_last_arg")),
      go_to(label("eval_dispatch")),
      
      "ev_appl_accum_last_arg",
      restore("argl"),
      assign("argl", list(op("adjoin_arg"),
                          reg("val"), reg("argl"))),
      restore("fun"),
      go_to(label("apply_dispatch")),

"compiled_apply",
  push_marker_to_stack(),
  assign("val", list(op("compiled_function_entry"), reg("fun"))),
  go_to(reg("val")),
      
      "apply_dispatch",
  test(list(op("is_compiled_function"), reg("fun"))),
  branch(label("compiled_apply")),
      test(list(op("is_primitive_function"),
                reg("fun"))),
      branch(label("primitive_apply")),
      test(list(op("is_compound_function"),
                reg("fun"))),
      branch(label("compound_apply")),
      go_to(label("unknown_function_type")),

      "primitive_apply",
      assign("val", list(op("apply_primitive_function"), 
                         reg("fun"), 
                         reg("argl"))),
      restore("continue"),
      go_to(reg("continue")),

      "compound_apply",
      assign("unev", list(op("function_parameters"), reg("fun"))),
      assign("env", list(op("function_environment"), reg("fun"))),
      assign("env", list(op("extend_environment"), 
                         reg("unev"), reg("argl"), reg("env"))),
      assign("comp", list(op("function_body"), reg("fun"))),
      push_marker_to_stack(),
      assign("continue", label("return_undefined")),
      go_to(label("eval_dispatch")),

      "return_undefined",
      assign("val", constant(undefined)),
      revert_stack_to_marker(),
      restore("continue"),
      go_to(reg("continue")),

      "ev_block",
      assign("comp", list(op("block_body"), reg("comp"))),
      assign("val", list(op("scan_out_declarations"), reg("comp"))),

      save("comp"), // temporarily store to comp
      assign("comp", list(op("list_of_unassigned"), reg("val"))),
      assign("env", list(op("extend_environment"), 
                         reg("val"), 
                         reg("comp"), 
                         reg("env"))),
      restore("comp"),
      go_to(label("eval_dispatch")),

"ev_sequence_start",
      assign("unev", list(op("sequence_statements"), reg("comp"))),
      save("continue"),

"ev_sequence",
      assign("comp", list(op("first_statement"), reg("unev"))),
      test(list(op("is_last_statement"), reg("unev"))),
      branch(label("ev_sequence_last_statement")),
      save("unev"),
      save("env"),
      assign("continue", label("ev_sequence_continue")),
      go_to(label("eval_dispatch")),
      
"ev_sequence_continue",
      restore("env"),
      restore("unev"),
      assign("unev", list(op("rest_statements"), reg("unev"))),
      go_to(label("ev_sequence")),
 
"ev_sequence_last_statement",
      restore("continue"),
      go_to(label("eval_dispatch")),

      "ev_conditional",
      save("comp"), // save expression for later
      save("env"),
      save("continue"),
      assign("continue", label("ev_conditional_decide")),
      assign("comp", list(op("conditional_predicate"), reg("comp"))),
      go_to(label("eval_dispatch")), // evaluate the predicate
      
      "ev_conditional_decide",
      restore("continue"),
      restore("env"),
      restore("comp"),
      test(list(op("is_falsy"), reg("val"))),
      branch(label("ev_conditional_alternative")),
      
      "ev_conditional_consequent",
      assign("comp", list(op("conditional_consequent"), reg("comp"))),
      go_to(label("eval_dispatch")),
      
      "ev_conditional_alternative",
      assign("comp", list(op("conditional_alternative"), reg("comp"))),
      go_to(label("eval_dispatch")),
      
      "ev_assignment",
      assign("unev", list(op("assignment_symbol"), reg("comp"))),
      save("unev"), // save variable for later
      assign("comp", list(op("assignment_value_expression"), reg("comp"))),
      save("env"),
      save("continue"),
      assign("continue", label("ev_assignment_1")),
      go_to(label("eval_dispatch")), // evaluate assignment value
      
      "ev_assignment_1",
      restore("continue"),
      restore("env"),
      restore("unev"),
      perform(list(op("assign_symbol_value"),
                   reg("unev"), reg("val"), reg("env"))),
      go_to(reg("continue")),
      
      "ev_function_declaration",
      assign("comp", list(op("function_decl_to_constant_decl"), reg("comp"))),
      
      "ev_declaration",
      assign("unev", list(op("declaration_symbol"),
                          reg("comp"))),
      save("unev"), // save variable for later
      assign("comp", list(op("declaration_value_expression"),
                          reg("comp"))),
      save("env"),
      save("continue"),
      assign("continue", label("ev_declaration_assign")),
      go_to(label("eval_dispatch")), // evaluate declaration value
      
      "ev_declaration_assign",
      restore("continue"),
      restore("env"),
      restore("unev"),
      perform(list(op("assign_symbol_value"),
                   reg("unev"), reg("val"), reg("env"))),
      assign("val", constant(undefined)),
      go_to(reg("continue")),
      
      // Error handling
      "unknown_component_type",
      assign("val", constant("Unknown expression type")),
      go_to(label("signal_error")),
      
      "unknown_function_type",
      restore("continue"), /// clean up stack (from apply_dispatch)
      assign("val", constant("Unknown function type")),
      go_to(label("signal_error")),
      
      "signal_error",
      perform(list(op("user_print"),
                   constant("EC-evaluator error:"), reg("comp"))),
      go_to(label("read_evaluate_print_loop")),
      
      "evaluator_done"
     );

const eceval =
    make_machine(list("comp", "env", "val", "fun",
                      "argl", "continue", "unev"),
                 eceval_operations,
                 eceval_controller);
