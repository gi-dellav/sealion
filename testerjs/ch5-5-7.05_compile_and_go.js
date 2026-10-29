// functions from SICP JS 4.1.1
function scan_out_declarations(component) {
    return is_sequence(component)
           ? accumulate(append,
                        null,
                        map(scan_out_declarations,
                            sequence_statements(component)))
           : is_declaration(component)
           ? list(declaration_symbol(component))
           : null;
}
function list_of_unassigned(symbols) {
    return map(symbol => "*unassigned*", symbols);
}
// functions from SICP JS 4.1.2
function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}
function is_literal(component) {
    return is_tagged_list(component, "literal");
}
function literal_value(component) {    
    return head(tail(component));
}
function make_literal(value) {
    return list("literal", value);
}
function is_name(component) {
    return is_tagged_list(component, "name");
}
function make_name(symbol) {
    return list("name", symbol);
}
function symbol_of_name(component) {
    return head(tail(component));
}
function is_assignment(component) {
    return is_tagged_list(component, "assignment");
}
function assignment_symbol(component) {
    return head(tail(head(tail(component))));
}
function assignment_value_expression(component) {
    return head(tail(tail(component)));
}
function is_declaration(component) {
    return is_tagged_list(component, "constant_declaration") ||
           is_tagged_list(component, "variable_declaration") ||
           is_tagged_list(component, "function_declaration");
}
function declaration_symbol(component) {
    return symbol_of_name(head(tail(component)));
}
function declaration_value_expression(component) {
    return head(tail(tail(component)));
}
function make_constant_declaration(name, value_expression) {
    return list("constant_declaration", name, value_expression);
}
function is_lambda_expression(component) {
    return is_tagged_list(component, "lambda_expression");
}
function lambda_parameter_symbols(component) {
    return map(symbol_of_name, head(tail(component)));
}
function lambda_body(component) {
    return head(tail(tail(component)));
}
function make_lambda_expression(parameters, body) {
    return list("lambda_expression", parameters, body);
}
function is_function_declaration(component) {	    
    return is_tagged_list(component, "function_declaration");
}
function function_declaration_name(component) {
    return list_ref(component, 1);
}
function function_declaration_parameters(component) {
    return list_ref(component, 2);
}
function function_declaration_body(component) {
    return list_ref(component, 3);
}
function function_decl_to_constant_decl(component) {
    return make_constant_declaration(
               function_declaration_name(component),
               make_lambda_expression(
                   function_declaration_parameters(component),
                   function_declaration_body(component)));
}
function is_return_statement(component) {
   return is_tagged_list(component, "return_statement");
}
function return_expression(component) {
   return head(tail(component));
}
function is_conditional(component) {
    return is_tagged_list(component, "conditional_expression") ||
           is_tagged_list(component, "conditional_statement");
}
function conditional_predicate(component) {
   return list_ref(component, 1);
}
function conditional_consequent(component) {
   return list_ref(component, 2);
}
function conditional_alternative(component) {
   return list_ref(component, 3);
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
function is_block(component) {
    return is_tagged_list(component, "block");
}
function block_body(component) {
    return head(tail(component));
}
function make_block(statement) {
    return list("block", statement);
}
function is_operator_combination(component) {	    
    return is_unary_operator_combination(component) ||
           is_binary_operator_combination(component);
}
function is_unary_operator_combination(component) {	    
    return is_tagged_list(component, "unary_operator_combination");
}
function is_binary_operator_combination(component) {	    
    return is_tagged_list(component, "binary_operator_combination");
}
function operator_symbol(component) {
    return list_ref(component, 1);
}
function first_operand(component) {
    return list_ref(component, 2);
}
function second_operand(component) {
    return list_ref(component, 3);
}
function make_application(function_expression, argument_expressions) {
    return list("application",
                function_expression, argument_expressions);
}
function operator_combination_to_application(component) {
    const operator = operator_symbol(component);
    return is_unary_operator_combination(component)
           ? make_application(make_name(operator),
                              list(first_operand(component)))
           : make_application(make_name(operator),
                              list(first_operand(component),
                                   second_operand(component)));
}
function is_application(component) {
   return is_tagged_list(component, "application");
}
function function_expression(component) {
   return head(tail(component));
}
function arg_expressions(component) {
   return head(tail(tail(component)));
}

// functions from SICP JS 4.1.3
function is_truthy(x) {
    return is_boolean(x) 
           ? x
           : error(x, "boolean expected, received");
}
function is_falsy(x) { return ! is_truthy(x); }
function make_function(parameters, body, env) {
    return list("compound_function", parameters, body, env);
}
function is_compound_function(f) {
    return is_tagged_list(f, "compound_function");
}
function function_parameters(f) { return list_ref(f, 1); }

function function_body(f) { return list_ref(f, 2); }

function function_environment(f) { return list_ref(f, 3); }
function make_return_value(content) {
    return list("return_value", content);
}
function is_return_value(value) {
    return is_tagged_list(value, "return_value");
}
function return_value_content(value) {
    return head(tail(value));
}
function enclosing_environment(env) { return tail(env); }

function first_frame(env) { return head(env); }

const the_empty_environment = null;
function make_frame(symbols, values) { return pair(symbols, values); }

function frame_symbols(frame) { return head(frame); }

function frame_values(frame) { return tail(frame); }
function extend_environment(symbols, vals, base_env) {
    return length(symbols) === length(vals)
           ? pair(make_frame(symbols, vals), base_env)
           : length(symbols) < length(vals)
           ? error("too many arguments supplied: " + 
                   stringify(symbols) + ", " + 
                   stringify(vals))
           : error("too few arguments supplied: " + 
                   stringify(symbols) + ", " + 
                   stringify(vals));
}
function lookup_symbol_value(symbol, env) {
    function env_loop(env) {
        function scan(symbols, vals) {
            return is_null(symbols)
                   ? env_loop(enclosing_environment(env))
                   : symbol === head(symbols)
                   ? head(vals)
                   : scan(tail(symbols), tail(vals));
        }
        if (env === the_empty_environment) {
            error(symbol, "unbound name");
        } else {
            const frame = first_frame(env);
            return scan(frame_symbols(frame), frame_values(frame));
        }
    }
    return env_loop(env);
}
function assign_symbol_value(symbol, val, env) {
    function env_loop(env) {
        function scan(symbols, vals) {
            return is_null(symbols)
                   ? env_loop(enclosing_environment(env))
                   : symbol === head(symbols)
                   ? set_head(vals, val)
                   : scan(tail(symbols), tail(vals));
        } 
        if (env === the_empty_environment) {
            error(symbol, "unbound name -- assignment");
        } else {
            const frame = first_frame(env);
            return scan(frame_symbols(frame), frame_values(frame));
        }
    }
    return env_loop(env);
}

// functions from SICP JS 4.1.4
function is_primitive_function(fun) {
    return is_tagged_list(fun, "primitive");
}

function primitive_implementation(fun) { return head(tail(fun)); }
const primitive_functions = list(
       list("head",    head             ),
       list("tail",    tail             ),
       list("pair",    pair             ),
       list("list",    list             ),
       list("is_null", is_null          ),
       list("display", display          ),
       list("error",   error            ),
       list("math_abs",math_abs         ),
       list("+",       (x, y) => x + y  ),
       list("-",       (x, y) => x - y  ),
       list("-unary",   x     =>   - x  ),
       list("*",       (x, y) => x * y  ),
       list("/",       (x, y) => x / y  ),
       list("%",       (x, y) => x % y  ),
       list("===",     (x, y) => x === y),
       list("!==",     (x, y) => x !== y),
       list("<",       (x, y) => x <   y),
       list("<=",      (x, y) => x <=  y),
       list(">",       (x, y) => x >   y),
       list(">=",      (x, y) => x >=  y),
       list("!",        x     =>   !   x)
       );
const primitive_function_symbols =
    map(head, primitive_functions);
const primitive_function_objects =
    map(fun => list("primitive", head(tail(fun))),
        primitive_functions);
const primitive_constants = list(list("undefined", undefined),
                                 list("Infinity",  Infinity),
                                 list("math_PI",   math_PI),
                                 list("math_E",    math_E),
                                 list("NaN",       NaN)
                                );
const primitive_constant_symbols =
    map(c => head(c), primitive_constants);
const primitive_constant_values =
    map(c => head(tail(c)), primitive_constants);
function apply_primitive_function(fun, arglist) {
    return apply_in_underlying_javascript(
               primitive_implementation(fun), arglist);
}
function setup_environment() {
    return extend_environment(append(primitive_function_symbols,
                                     primitive_constant_symbols),
                              append(primitive_function_objects, 
                                     primitive_constant_values),
                              the_empty_environment);
}
const the_global_environment = setup_environment();

function get_register(machine, reg_name) {
    return machine("get_register")(reg_name);
}
function type(instruction) { return head(instruction); }
function lookup_prim(symbol, operations) {
    const val = assoc(symbol, operations);
    return is_undefined(val)
           ? error(symbol, "unknown operation -- assemble")
           : head(tail(val));
}
function lookup_label(labels, label_name) {
    const val = assoc(label_name, labels);
    return is_undefined(val)
           ? error(label_name, "undefined label -- assemble")
           : tail(val);
}
function make_primitive_exp_ef(exp, machine, labels) {
    if (is_constant_exp(exp)) {
        const c = constant_exp_value(exp);
        return () => c;
    } else if (is_label_exp(exp)) {
        const insts = lookup_label(labels, label_exp_label(exp));
        return () => insts;
    } else if (is_register_exp(exp)) {
        const r = get_register(machine, register_exp_reg(exp));
        return () => get_contents(r); 
    } else {
        error(exp, "unknown expression type -- assemble");
    }
}
function make_operation_exp_ef(exp, machine, labels, operations) {
    const op = lookup_prim(operation_exp_op(exp), operations);
    const afuns = map(e => make_primitive_exp_ef(e, machine, labels),
                      operation_exp_operands(exp));
    return () => apply_in_underlying_javascript(
                     op, map(f => f(), afuns));
}
function assign(register_name, source) {
    return list("assign", register_name, source);
}
function assign_reg_name(assign_instruction) {
    return head(tail(assign_instruction));
}
function assign_value_exp(assign_instruction) { 
    return head(tail(tail(assign_instruction)));
}
function make_assign_ef(inst, machine, labels, operations, pc) {
    const target = get_register(machine, assign_reg_name(inst));
    const value_exp = assign_value_exp(inst);
    const value_fun =
        is_operation_exp(value_exp)
        ? make_operation_exp_ef(value_exp, machine, labels, operations)
        : make_primitive_exp_ef(value_exp, machine, labels);
    return () => {
               set_contents(target, value_fun());
               advance_pc(pc); 
           };
}
function advance_pc(pc) {
    set_contents(pc, tail(get_contents(pc))); 
}
function make_test_ef(inst, machine, labels, operations, flag, pc) {
    const condition = test_condition(inst);
    if (is_operation_exp(condition)) {
        const condition_fun = make_operation_exp_ef(
                                  condition, machine, 
                                  labels, operations);
        return () => {
                   set_contents(flag, condition_fun());
                   advance_pc(pc); 
               };
    } else {
        error(inst, "bad test instruction -- assemble");
    }
}
function test(condition) { return list("test", condition); }

function test_condition(test_instruction) {
    return head(tail(test_instruction)); 
}
function make_branch_ef(inst, machine, labels, flag, pc) {
    const dest = branch_dest(inst);
    if (is_label_exp(dest)) {
        const insts = lookup_label(labels, label_exp_label(dest));
        return () => {
                   if (get_contents(flag)) {
                       set_contents(pc, insts);
                   } else {
                       advance_pc(pc);
                   }
               };
    } else {
        error(inst, "bad branch instruction -- assemble");
    }
}
function branch(label) { return list("branch", label); }

function branch_dest(branch_instruction) {
    return head(tail(branch_instruction)); 
}
function reg(name) { return list("reg", name); }

function is_register_exp(exp) { return is_tagged_list(exp, "reg"); }

function register_exp_reg(exp) { return head(tail(exp)); }

function constant(value) { return list("constant", value); }

function is_constant_exp(exp) {
    return is_tagged_list(exp, "constant");
}

function constant_exp_value(exp) { return head(tail(exp)); }

function label(name) { return list("label", name); }

function is_label_exp(exp) { return is_tagged_list(exp, "label"); }

function label_exp_label(exp) { return head(tail(exp)); }
function op(name) { return list("op", name); }

function is_operation_exp(exp) {
    return is_pair(exp) && is_tagged_list(head(exp), "op");
}

function operation_exp_op(op_exp) { return head(tail(head(op_exp))); }

function operation_exp_operands(op_exp) { return tail(op_exp); }
function make_go_to_ef(inst, machine, labels, pc) {
    const dest = go_to_dest(inst);
    if (is_label_exp(dest)) {
        const insts = lookup_label(labels, label_exp_label(dest));
        return () => set_contents(pc, insts);
    } else if (is_register_exp(dest)) {
        const reg = get_register(machine, register_exp_reg(dest));
        return () => set_contents(pc, get_contents(reg));
    } else {
        error(inst, "bad go_to instruction -- assemble");
    }
}
function go_to(label) { return list("go_to", label); }

function go_to_dest(go_to_instruction) { 
    return head(tail(go_to_instruction)); 
}
function pop(stack) {
    return stack("pop");
}
function push(stack, value) {
    return stack("push")(value);
}
function make_save_ef(inst, machine, stack, pc) {
    const reg = get_register(machine, stack_inst_reg_name(inst));
    return () => {
               push(stack, get_contents(reg));
               advance_pc(pc);
           };
}
function make_restore_ef(inst, machine, stack, pc) {
    const reg = get_register(machine, stack_inst_reg_name(inst));
    return () => {
               set_contents(reg, pop(stack));
               advance_pc(pc); 
           };
}
function save(reg) { return list("save", reg); }

function restore(reg) { return list("restore", reg); }

function stack_inst_reg_name(stack_instruction) {
    return head(tail(stack_instruction));
}
function make_perform_ef(inst, machine, labels, operations, pc) {
    const action = perform_action(inst);
    if (is_operation_exp(action)) {
        const action_fun = make_operation_exp_ef(action, machine,
                                                 labels, operations);
        return () => { 
                   action_fun(); 
                   advance_pc(pc); 
               };
    } else {
        error(inst, "bad perform instruction -- assemble");
    }
}
function perform(action) { return list("perform", action); }

function perform_action(perform_instruction) {
    return head(tail(perform_instruction));
}
function make_execution_function(inst, labels, machine, 
                                 pc, flag, stack, ops) {
    return type(inst) === "assign"
           ? make_assign_ef(inst, machine, labels, ops, pc)
           : type(inst) === "test"
           ? make_test_ef(inst, machine, labels, ops, flag, pc)
           : type(inst) === "branch"
           ? make_branch_ef(inst, machine, labels, flag, pc)
           : type(inst) === "go_to"
           ? make_go_to_ef(inst, machine, labels, pc)
           : type(inst) === "save"
           ? make_save_ef(inst, machine, stack, pc)
           : type(inst) === "restore"
           ? make_restore_ef(inst, machine, stack, pc)
           : type(inst) === "push_marker_to_stack"
           ? make_push_marker_to_stack_ef(machine, stack, pc)
           : type(inst) === "revert_stack_to_marker"
           ? make_revert_stack_to_marker_ef(machine, stack, pc)
           : type(inst) === "perform"
           ? make_perform_ef(inst, machine, labels, ops, pc)
           : error(inst, "unknown instruction type -- assemble");
}
function make_inst(inst_controller_instruction) {
    return pair(inst_controller_instruction, null);
}
function inst_controller_instruction(inst) {
    return head(inst);
}
function inst_execution_fun(inst) {
    return tail(inst);
}
function set_inst_execution_fun(inst, fun) {
    set_tail(inst, fun);
}
function update_insts(insts, labels, machine) {
    const pc = get_register(machine, "pc");
    const flag = get_register(machine, "flag");
    const stack = machine("stack");
    const ops = machine("operations");
    return for_each(inst => set_inst_execution_fun(
                                inst,
                                make_execution_function(
                                    inst_controller_instruction(inst),
                                    labels, machine, pc,
                                    flag, stack, ops)),
                    insts);
}
function make_label_entry(label_name, insts) {
    return pair(label_name, insts);
}
function extract_labels(controller, receive) {
    return is_null(controller)
           ? receive(null, null)
           : extract_labels(
                 tail(controller),
                 (insts, labels) => {
                   const next_element = head(controller);
                   return is_string(next_element)
                          ? receive(insts,
                                    pair(make_label_entry(next_element,
                                                          insts),
                                         labels))
                          : receive(pair(make_inst(next_element),
                                         insts),
                                    labels);
                 });
}
function assemble(controller, machine) {
    return extract_labels(controller,
                          (insts, labels) => {
                              update_insts(insts, labels, machine);
                              return insts;
                          });
}
function make_stack() {
    let stack = null;
    let frame = null;
    function push_marker() {
        frame = pair(stack, frame);
        return "done";
    }
    function pop_marker() {
        stack = head(frame);
        frame = tail(frame);
        return "done";
    }
    function push(x) { 
        stack = pair(x, stack); 
        return "done";
    }
    function pop() {
        if (is_null(stack)) {
            error("empty stack -- pop");
        } else {
            const top = head(stack);
            stack = tail(stack);
            return top;
        }
    }
    function initialize() {
        stack = null;
        return "done";
    }
    function dispatch(message) {
        return message === "push"
               ? push
               : message === "pop"
               ? pop()
               : message === "push_marker"
               ? push_marker()
               : message === "pop_marker"
               ? pop_marker()
               : message === "initialize"
               ? initialize()
               : error(message, "unknown request -- stack");
    }
    return dispatch;
}

function make_push_marker_to_stack_ef(machine, stack, pc) {
    return () => {
                   push_marker(stack);
                   advance_pc(pc);
                 };
}
function make_revert_stack_to_marker_ef(machine, stack, pc) {
    return () => {
                   pop_marker(stack);
                   advance_pc(pc); 
                 };
}

function push_marker_to_stack() { return list("push_marker_to_stack"); }
function revert_stack_to_marker() { return list("revert_stack_to_marker"); }

function pop_marker(stack) {
    return stack("pop_marker");
}
function push_marker(stack) {
    return stack("push_marker");
}
function make_register(name) {
    let contents = "*unassigned*";
    function dispatch(message) {
        return message === "get"
               ? contents
               : message === "set"
               ? value => { contents = value; }
               : error(message, "unknown request -- make_register");
    }
    return dispatch;
}
function lookup(key, table) {
    const record = assoc(key, tail(table));
    return is_undefined(record)
           ? undefined
           : tail(record);
}
function assoc(key, records) {
    return is_null(records)
           ? undefined
           : equal(key, head(head(records)))
           ? head(records)
           : assoc(key, tail(records));
}
function get_contents(register) {
    return register("get");
}
function set_contents(register, value) {
    return register("set")(value);
}
function make_new_machine() {
    const pc = make_register("pc");
    const flag = make_register("flag");
    const stack = make_stack();
    let the_instruction_sequence = null;
    let the_ops = list(list("initialize_stack", () => stack("initialize")));
    let register_table = list(list("pc", pc), list("flag", flag));
    function allocate_register(name) {
        if (is_undefined(assoc(name, register_table))) {
            register_table = pair(list(name, make_register(name)),
                                  register_table);
        } else {
            error(name, "multiply defined register");
        }
        return "register allocated";
    }
    function lookup_register(name) {
        const val = assoc(name, register_table);
        return is_undefined(val)
               ? error(name, "unknown register")
               : head(tail(val));
    }
    function execute() {
        const insts = get_contents(pc);
        if (is_null(insts)) {
            return "done";
        } else {
            inst_execution_fun(head(insts))();
            return execute();
        }
    }
    function dispatch(message) {
        function start() {
            set_contents(pc, the_instruction_sequence);
            return execute();
        }
        return message === "start"
               ? start()
               : message === "install_instruction_sequence"
               ? seq => { the_instruction_sequence = seq; }
               : message === "allocate_register"
               ? allocate_register
               : message === "get_register"
               ? lookup_register
               : message === "install_operations"
               ? ops => { the_ops = append(the_ops, ops); }
               : message === "stack"
               ? stack
               : message === "operations"
               ? the_ops
               : error(message, "unknown request -- machine");
    }
    return dispatch;
}
function make_machine(register_names, ops, controller) {
    const machine = make_new_machine();
    for_each(register_name => 
               machine("allocate_register")(register_name), 
             register_names);
    machine("install_operations")(ops);
    machine("install_instruction_sequence")
           (assemble(controller, machine));
    return machine;
}
function start(machine) {
    return machine("start");
}
function get_register_contents(machine, register_name) {
    return get_contents(get_register(machine, register_name));
}
function set_register_contents(machine, register_name, value) {
    set_contents(get_register(machine, register_name), value);
    return "done";
}
function compile(component, target, linkage) {
    return is_literal(component)
           ? compile_literal(component, target, linkage)
           : is_name(component)
           ? compile_name(component, target, linkage)
           : is_application(component)
           ? compile_application(component, target, linkage)
           : is_operator_combination(component)
           ? compile(operator_combination_to_application(component),
                     target, linkage)
           : is_conditional(component)
           ? compile_conditional(component, target, linkage)
           : is_lambda_expression(component)
           ? compile_lambda_expression(component, target, linkage)
           : is_sequence(component)
           ? compile_sequence(sequence_statements(component),
                              target, linkage)
           : is_block(component)
           ? compile_block(component, target, linkage)
           : is_return_statement(component)
           ? compile_return_statement(component, target, linkage)
           : is_function_declaration(component)
           ? compile(function_decl_to_constant_decl(component),
                     target, linkage)
           : is_declaration(component)
           ? compile_declaration(component, target, linkage)
           : is_assignment(component)
           ? compile_assignment(component, target, linkage)
           : error(component, "unknown component type -- compile");
}
function make_instruction_sequence(needs, modifies, instructions) {
    return list(needs, modifies, instructions);
}
function compile_linkage(linkage) {
    return linkage === "return"
           ? make_instruction_sequence(list("continue"), null,
                                       list(go_to(reg("continue"))))
           : linkage === "next"
           ? make_instruction_sequence(null, null, null)
           : make_instruction_sequence(null, null, 
                                       list(go_to(label(linkage))));
}
function end_with_linkage(linkage, instruction_sequence) {
    return preserving(list("continue"),
                      instruction_sequence,
                      compile_linkage(linkage));
}
function compile_literal(component, target, linkage) {
    const literal = literal_value(component);
    return end_with_linkage(linkage,
               make_instruction_sequence(null, list(target),
                   list(assign(target, constant(literal)))));
}
function compile_name(component, target, linkage) {
    const symbol = symbol_of_name(component);
    return end_with_linkage(linkage,
               make_instruction_sequence(list("env"), list(target),
                   list(assign(target,
                               list(op("lookup_symbol_value"),
                                    constant(symbol),
                                    reg("env"))))));
}
function compile_assignment(component, target, linkage) {
    return compile_assignment_declaration(
               assignment_symbol(component),
               assignment_value_expression(component),
               reg("val"),
               target, linkage);
}

function compile_declaration(component, target, linkage) {
    return compile_assignment_declaration(
               declaration_symbol(component),
               declaration_value_expression(component),
               constant(undefined),
               target, linkage);
}
function compile_assignment_declaration(
             symbol, value_expression, final_value,
             target, linkage) {
    const get_value_code = compile(value_expression, "val", "next");
    return end_with_linkage(linkage,
               preserving(list("env"),
                   get_value_code,
                   make_instruction_sequence(list("env", "val"),
                                             list(target),
                       list(perform(list(op("assign_symbol_value"),
                                         constant(symbol),
                                         reg("val"),
                                         reg("env"))),
                            assign(target, final_value)))));
}
let label_counter = 0;

function new_label_number() {
    label_counter = label_counter + 1;
    return label_counter;
}
function make_label(string) {
    return string + stringify(new_label_number());
}
function compile_conditional(component, target, linkage) {
    const t_branch = make_label("true_branch");
    const f_branch = make_label("false_branch");
    const after_cond = make_label("after_cond");
    const consequent_linkage =
            linkage === "next" ? after_cond : linkage;
    const p_code = compile(conditional_predicate(component),
                           "val", "next");
    const c_code = compile(conditional_consequent(component),
                           target, consequent_linkage);
    const a_code = compile(conditional_alternative(component),
                           target, linkage);
    return preserving(list("env", "continue"),
             p_code,
             append_instruction_sequences(
               make_instruction_sequence(list("val"), null,
                 list(test(list(op("is_falsy"), reg("val"))),
                      branch(label(f_branch)))),
               append_instruction_sequences(
                 parallel_instruction_sequences(
                   append_instruction_sequences(t_branch, c_code),
                   append_instruction_sequences(f_branch, a_code)),
                 after_cond)));
}
function compile_sequence(seq, target, linkage) {
    return is_empty_sequence(seq)
           ? compile_literal(make_literal(undefined), target, linkage)
           : is_last_statement(seq) ||
                 is_return_statement(first_statement(seq))
           ? compile(first_statement(seq), target, linkage)
           : preserving(list("env", "continue"),
                 compile(first_statement(seq), target, "next"),
                 compile_sequence(rest_statements(seq), 
                                  target, linkage));
}
function make_compiled_function(entry, env) {
    return list("compiled_function", entry, env);
}
function is_compiled_function(fun) {
    return is_tagged_list(fun, "compiled_function");
}
function compiled_function_entry(c_fun) {
    return head(tail(c_fun));
}
function compiled_function_env(c_fun) {
    return head(tail(tail(c_fun)));
}
function compile_lambda_expression(exp, target, linkage) {
    const fun_entry = make_label("entry");
    const after_lambda = make_label("after_lambda");
    const lambda_linkage =
            linkage === "next" ? after_lambda : linkage;
    return append_instruction_sequences(
               tack_on_instruction_sequence(
                   end_with_linkage(lambda_linkage,
                       make_instruction_sequence(list("env"),
                                                 list(target),
                           list(assign(target,
                                    list(op("make_compiled_function"),
                                         label(fun_entry),
                                         reg("env")))))),
                   compile_lambda_body(exp, fun_entry)),
               after_lambda);
}
function append_return_undefined(body) {
    return list("sequence", list(body,
                                 list("return_statement",
                                      list("literal", undefined))));
}
function compile_lambda_body(exp, fun_entry) {
    const params  = lambda_parameter_symbols(exp);
    return append_instruction_sequences(
        make_instruction_sequence(list("env", "fun", "argl"),
                                  list("env"),
            list(fun_entry,
                 assign("env",
                        list(op("compiled_function_env"),
                             reg("fun"))),
                 assign("env",
                        list(op("extend_environment"),
                             constant(params),
                             reg("argl"),
                             reg("env"))))),
        compile(append_return_undefined(lambda_body(exp)),
                "val", "next"));
}
function compile_return_statement(stmt, target, linkage) {
    return append_instruction_sequences(
               make_instruction_sequence(null, list("continue"),
                   list(revert_stack_to_marker(),
                        restore("continue"))),
               compile(return_expression(stmt), "val", "return"));
}
function compile_block(stmt, target, linkage) {
    const body = block_body(stmt);
    const locals = scan_out_declarations(body);
    const unassigneds = list_of_unassigned(locals);
    return append_instruction_sequences(
               make_instruction_sequence(list("env"), list("env"),
                   list(assign("env", list(op("extend_environment"),
                                           constant(locals),
                                           constant(unassigneds),
                                           reg("env"))))),
               compile(body, target, linkage));
}
function compile_application(exp, target, linkage) {
    const fun_code = compile(function_expression(exp), "fun", "next");
    const argument_codes = map(arg => compile(arg, "val", "next"),
                               arg_expressions(exp));
    return preserving(list("env", "continue"),
                      fun_code,
                      preserving(list("fun", "continue"),
                          construct_arglist(argument_codes),
                          compile_function_call(target, linkage)));
}
function construct_arglist(arg_codes) {
    if (is_null(arg_codes)) {
        return make_instruction_sequence(null, list("argl"),
                   list(assign("argl", constant(null))));
    } else {
        const rev_arg_codes = reverse(arg_codes);
        const code_to_get_last_arg =
            append_instruction_sequences(
                head(rev_arg_codes),
                make_instruction_sequence(list("val"), list("argl"),
                    list(assign("argl", 
                                list(op("list"), reg("val"))))));
        return is_null(tail(rev_arg_codes))
               ? code_to_get_last_arg
               : preserving(list("env"),
                     code_to_get_last_arg,
                     code_to_get_rest_args(tail(rev_arg_codes)));
    }
}
function code_to_get_rest_args(arg_codes) {
    const code_for_next_arg =
        preserving(list("argl"),
            head(arg_codes),
            make_instruction_sequence(list("val", "argl"), list("argl"),
                list(assign("argl", list(op("pair"),
                                         reg("val"), reg("argl"))))));
    return is_null(tail(arg_codes))
           ? code_for_next_arg
           : preserving(list("env"),
                        code_for_next_arg,
                        code_to_get_rest_args(tail(arg_codes)));
}
function compile_function_call(target, linkage) {
    const primitive_branch = make_label("primitive_branch");
    const compiled_branch = make_label("compiled_branch");
    const after_call = make_label("after_call");
    const compiled_linkage = linkage === "next" ? after_call : linkage;
    return append_instruction_sequences(
        make_instruction_sequence(list("fun"), null,
            list(test(list(op("is_primitive_function"), reg("fun"))),
                 branch(label(primitive_branch)))),
        append_instruction_sequences(
            parallel_instruction_sequences(
                append_instruction_sequences(
                    compiled_branch,
                    compile_fun_appl(target, compiled_linkage)),
                append_instruction_sequences(
                    primitive_branch,
                    end_with_linkage(linkage,
                        make_instruction_sequence(list("fun", "argl"),
                                                  list(target),
                            list(assign(
                                   target,
                                   list(op("apply_primitive_function"),
                                        reg("fun"), reg("argl")))))))),
            after_call));
}
const all_regs = list("env", "fun", "val", "argl", "continue");
function compile_fun_appl(target, linkage) {
    const fun_return = make_label("fun_return");
    return target === "val" && linkage !== "return"
           ? make_instruction_sequence(list("fun"), all_regs,
                 list(assign("continue", label(linkage)),
                      save("continue"),
                      push_marker_to_stack(),
                      assign("val", list(op("compiled_function_entry"),
                                         reg("fun"))),
                      go_to(reg("val"))))
           : target !== "val" && linkage !== "return"
           ? make_instruction_sequence(list("fun"), all_regs,
                 list(assign("continue", label(fun_return)),
                      save("continue"),
                      push_marker_to_stack(),
                      assign("val", list(op("compiled_function_entry"),
                                         reg("fun"))),
                      go_to(reg("val")),
                      fun_return,
                      assign(target, reg("val")),
                      go_to(label(linkage))))
           : target === "val" && linkage === "return"
           ? make_instruction_sequence(list("fun", "continue"),
                                       all_regs,
                 list(save("continue"),
                      push_marker_to_stack(),
                      assign("val", list(op("compiled_function_entry"),
                                         reg("fun"))),
                      go_to(reg("val"))))
           : // $\texttt{target !== "val" \&\& linkage === "return"}$
             error(target, "return linkage, target not val -- compile");
}
function registers_needed(s) {
    return is_string(s) ? null : head(s);
}
function registers_modified(s) {
    return is_string(s) ? null : head(tail(s));
}
function instructions(s) {
    return is_string(s) ? list(s) : head(tail(tail(s)));
}
function needs_register(seq, reg) {
    return ! is_null(member(reg, registers_needed(seq)));
}
function modifies_register(seq, reg) {
    return ! is_null(member(reg, registers_modified(seq)));
}
function append_instruction_sequences(seq1, seq2) {
    return make_instruction_sequence(
               list_union(registers_needed(seq1),
                          list_difference(registers_needed(seq2),
                                          registers_modified(seq1))),
               list_union(registers_modified(seq1),
                          registers_modified(seq2)),
               append(instructions(seq1), instructions(seq2)));
}
function list_union(s1, s2) {
    return is_null(s1)
           ? s2
           : is_null(member(head(s1), s2))
           ? pair(head(s1), list_union(tail(s1), s2))
           : list_union(tail(s1), s2);
}
function list_difference(s1, s2) {
    return is_null(s1)
           ? null
           : is_null(member(head(s1), s2))
           ? pair(head(s1), list_difference(tail(s1), s2))
           : list_difference(tail(s1), s2);
}
function preserving(regs, seq1, seq2) {
    if (is_null(regs)) {
        return append_instruction_sequences(seq1, seq2);
    } else {
        const first_reg = head(regs);
        return needs_register(seq2, first_reg) &&
               modifies_register(seq1, first_reg)
               ? preserving(tail(regs),
                     make_instruction_sequence(
                         list_union(list(first_reg),
                                    registers_needed(seq1)),
                         list_difference(registers_modified(seq1),
                                         list(first_reg)),
                         append(list(save(first_reg)),
                                append(instructions(seq1),
                                       list(restore(first_reg))))),
                     seq2)
               : preserving(tail(regs), seq1, seq2);
    }
}
function tack_on_instruction_sequence(seq, body_seq) {
    return make_instruction_sequence(
               registers_needed(seq),
               registers_modified(seq),
               append(instructions(seq), instructions(body_seq)));
}
function parallel_instruction_sequences(seq1, seq2) {
    return make_instruction_sequence(
               list_union(registers_needed(seq1),
                          registers_needed(seq2)),
               list_union(registers_modified(seq1),
                          registers_modified(seq2)),
               append(instructions(seq1), instructions(seq2)));
}
function user_print(prompt_string, object) {
   function to_string(object) {
       return is_compound_function(object)
              ? "<compound-function>"
              : is_compiled_function(object)
              ? "<compiled-function>"
              : is_primitive_function(object)
              ? "<primitive-function>"
              : is_pair(object)
              ? "[" + to_string(head(object)) + ", "
                    + to_string(tail(object)) + "]"
              : stringify(object);
    }
    display(prompt_string + "\n" + to_string(object) + "\n----------------------------");
}
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
function stringify_list(xs) {
    return is_null(member(head(xs),
                   list("assign", "perform", "op", "label", "branch",
                        "go_to", "save", "restore", "reg", "constant")))
           ? "list(" + comma_separated(xs) + ")"
           : head(xs) + "(" + comma_separated(tail(xs)) + ")";
}
function comma_separated(elements) {
    return accumulate((s, acc) => stringify_instruction(s) +
                                  (acc === "" ? "" : ", " + acc),
                      "", elements);
}
function stringify_instruction(x) {
    return is_string(x) || is_number(x) || is_undefined(x) || is_null(x)
           ? stringify(x)
           : stringify_list(x);
}
function display_instructions(instructions) {
    return for_each(i => {display(stringify_instruction(i) + ","); },
                    instructions);
}
function compile_and_go(program) {
    const instrs = assemble(instructions(compile(program,
                                                 "val", "return")),
                            eceval);
    const toplevel_names = scan_out_declarations(program);
    const unassigneds = list_of_unassigned(toplevel_names);
    set_current_environment(extend_environment(
                               toplevel_names,
                               unassigneds, 
                               the_global_environment));
    set_register_contents(eceval, "val", instrs);
    set_register_contents(eceval, "flag", true);
    return start(eceval);
}

compile_and_go(parse(`
function factorial(n) {
    return n === 1
           ? 1
           : factorial(n - 1) * n;
}
                     `));
