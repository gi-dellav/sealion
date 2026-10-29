function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
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
function pop(stack) {
    return stack("pop");
}
function push(stack, value) {
    return stack("push")(value);
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
function go_to(label) { return list("go_to", label); }

function go_to_dest(go_to_instruction) { 
    return head(tail(go_to_instruction)); 
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
// TYPED POINTERS

const NUMBER_TYPE = "number";
const BOOL_TYPE = "bool";
const STRING_TYPE = "string";
const PTR_TYPE = "ptr";
const PROG_TYPE = "prog";
const NULL_TYPE = "null";
const UNDEFINED_TYPE = "undefined";
const NO_VALUE_YET_TYPE = "*unassigned*";
const BROKEN_HEART_TYPE = "broken_heart";

function make_ptr_ptr(idx) {
    return pair(PTR_TYPE, idx);
}

function make_null_ptr() {
    return pair(NULL_TYPE, null);
}

function make_no_value_yet_ptr() {
    return pair(NO_VALUE_YET_TYPE, null);
}

function make_prog_ptr(idx) {
    return pair(PROG_TYPE, idx);
}

function make_broken_heart_ptr() {
    return pair(BROKEN_HEART_TYPE, null);
}

function get_elem_type(elem) {
    return is_number(elem) ? NUMBER_TYPE :
        is_boolean(elem) ? BOOL_TYPE :
        is_string(elem) ? STRING_TYPE :
        is_null(elem) ? NULL_TYPE :
        is_undefined(elem) ? UNDEFINED_TYPE :
        error(elem, "invalid typed elem");
}

function wrap_ptr(elem) {
    return pair(get_elem_type(elem), elem);
}

function unwrap_ptr(ptr) {
    return tail(ptr);
}

function is_ptr(ptr) {
    return is_pair(ptr) &&
        !is_pair(head(ptr)) &&
        !is_pair(tail(ptr)) &&
        (head(ptr) === NUMBER_TYPE ||
        head(ptr) === BOOL_TYPE ||
        head(ptr) === STRING_TYPE ||
        head(ptr) === PTR_TYPE ||
        head(ptr) === NULL_TYPE ||
        head(ptr) === UNDEFINED_TYPE ||
        head(ptr) === PROG_TYPE ||
        head(ptr) === NO_VALUE_YET_TYPE ||
        head(ptr) === BROKEN_HEART_TYPE);
}

function is_number_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === NUMBER_TYPE;
}

function is_bool_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === BOOL_TYPE;
}

function is_string_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === STRING_TYPE;
}

function is_ptr_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === PTR_TYPE;
}

function is_null_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === NULL_TYPE;
}

function is_undefined_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === UNDEFINED_TYPE;
}

function is_prog_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === PROG_TYPE;
}

function is_no_value_yet_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === NO_VALUE_YET_TYPE;
}

function is_broken_heart_ptr(ptr) {
    return is_ptr(ptr) && head(ptr) === BROKEN_HEART_TYPE;
}

// Primitive functions and constants

const primitive_function_names_arities = list(
       pair("display", 1),
       pair("error", 1),
       pair("+", 2),
       pair("-", 2),
       pair("*", 2),
       pair("/", 2),
       pair("%", 2),
       pair("===", 2),
       pair("!==", 2),
       pair("<", 2),
       pair("<=", 2),
       pair(">", 2),
       pair(">=", 2),
       pair("!", 1),
       pair("||", 2),
       pair("&&", 2)
);

const primitive_constants = list(
       list("undefined", undefined),
       list("math_PI"  , math_PI)
      );
       
function make_primitive_function(impl) {
    return list("primitive", impl);
}

function setup_environment() {
    const primitive_function_names =
        map(head, primitive_function_names_arities);
    const primitive_function_values =
        map(name => pair(make_primitive_function(name), false),
            primitive_function_names);
    const primitive_constant_names =
        map(head, primitive_constants);
    const primitive_constant_values =
        map(f => pair(head(tail(f)), false),
            primitive_constants);
    return pair(pair(
               append(primitive_function_names, 
                      primitive_constant_names),
               append(primitive_function_values, 
                      primitive_constant_values)),
               null);
}

function flatten_list_to_vectors(the_heads, the_tails, lst,
                                 make_ptr_fn, starting_index) {
    let free = starting_index;
    function helper(lst) {
        if (!is_pair(lst)) {
            return wrap_ptr(lst);
        } else {
            const index = free;
            free = free + 1;
            const elem = head(lst);
            the_heads[index] = helper(elem);
            the_tails[index] = helper(tail(lst));
            return make_ptr_fn(index);
        }
    }
    helper(lst);
    return free;
}

// MACHINE
function get_contents(register) {
    return register("get");
}

function set_contents(register, value) {
    return register("set")(value);
}

function make_stack() {
    let stack = null;

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
               : message === "initialize"
               ? initialize()
               : message === "stack"
               ? stack
               : error(message, "unknown request -- stack");
    }

    return dispatch;
}

function make_new_machine() {
    const SIZE = make_register("SIZE");
    const pc = make_register("pc");
    const flag = make_register("flag");
    const stack = make_stack();
    const stack_reassign_proc = make_register("stack_reassign_proc");
    const free = make_register("free");
    const root = make_register("root");
    const root_populate_proc = make_register("root_populate_proc");
    const root_restore_proc = make_register("root_restore_proc");
    const gc_registers = list(
        list("free", free),
        list("scan", make_register("scan")),
        list("old", make_register("old")),
        list("new", make_register("new")),
        list("relocate_continue", make_register("relocate_continue")),
        list("temp", make_register("temp")),
        list("oldht", make_register("oldht"))
    );
    const exp = make_register("exp");
    const env = make_register("env");
    const evaluator_registers = list(
        list("exp", exp),
        list("env", env),
        list("val", make_register("val")),
        list("continue", make_register("continue")),
        list("fun", make_register("fun")),
        list("argl", make_register("argl")),
        list("unev", make_register("unev")),
        list("fun", make_register("fun"))
    );
    const aux_registers = list(
        list("res", make_register("val")),
        list("err", make_register("err")),
        list("a", make_register("a")),
        list("b", make_register("b")),
        list("c", make_register("c")),
        list("d", make_register("d")),
        list("e", make_register("e")),
        list("f", make_register("f"))
    );
    const the_heads = make_register("the_heads");
    const the_tails = make_register("the_tails");
    set_contents(the_heads, make_vector());
    set_contents(the_tails, make_vector());
    const new_heads = make_register("new_heads");
    const new_tails = make_register("new_tails");
    set_contents(new_heads, make_vector());
    set_contents(new_tails, make_vector());
    const prog_heads = make_register("prog_heads");
    const prog_tails = make_register("prog_tails");
    let the_instruction_sequence = null;
    let the_ops = list(list("initialize_stack",
                            () => stack("initialize")));
    the_ops = append(the_ops, vector_ops);
    let register_table =
          list(list("SIZE", SIZE),
               list("pc", pc),
               list("flag", flag),
               list("root", root),
               list("root_populate_proc", root_populate_proc),
               list("root_restore_proc", root_restore_proc),
               list("stack_reassign_proc", stack_reassign_proc),
               list("the_heads", the_heads),
               list("the_tails", the_tails),
               list("new_heads", new_heads),
               list("new_tails", new_tails),
               list("prog_heads", prog_heads),
               list("prog_tails", prog_tails));
    register_table = append(register_table, gc_registers);
    register_table = append(register_table, evaluator_registers);
    register_table = append(register_table, aux_registers);

    function start() {
        const root_registers = 
                append(aux_registers, evaluator_registers);
        set_contents(pc, the_instruction_sequence);
        set_contents(free,
                     make_ptr_ptr(flatten_list_to_vectors(
                                    the_heads("get"),
                                    the_tails("get"),
                                    setup_environment(), 
                                    make_ptr_ptr,
                                    length(root_registers))));
        set_contents(env, make_ptr_ptr(length(root_registers)));
        function root_populate_proc_fn() {
            const root_ptr = free("get");
            root("set")(root_ptr);
            let register_list = root_registers;
            while (!is_null(register_list)) {
                const content = head(tail(head(register_list)))("get");
                const index = unwrap_ptr(free("get"));
                the_heads("get")[index] = 
                  content === "*unassigned*"
                  ? make_null_ptr() : content;
                free("set")(make_ptr_ptr(index + 1));
                the_tails("get")[index] = free("get");
                register_list = tail(register_list);
            }
            the_tails("get")[unwrap_ptr(free("get")) - 1] =
              make_null_ptr();
        }
        function root_restore_proc_fn() {
            let root_ptr = root("get");
            let register_list = root_registers;
            while (!is_null(register_list)) {
                const index = unwrap_ptr(root_ptr);
                const value = the_heads("get")[index];
                head(tail(head(register_list)))("set")(value);
                root_ptr = the_tails("get")[index];
                register_list = tail(register_list);
            }
        }
        function stack_reassign_proc_fn() {
            let local_stack = stack("stack");
            while (!is_null(local_stack)) {
                const value = head(local_stack);
                if (is_ptr_ptr(value)) {
                    const index = unwrap_ptr(value);
                    const new_ptr = the_tails("get")[index];
                    set_head(local_stack, new_ptr);
                } else {}
                local_stack = tail(local_stack);
            }
        }
        set_contents(root_populate_proc, root_populate_proc_fn);
        set_contents(root_restore_proc, root_restore_proc_fn);
        set_contents(stack_reassign_proc, stack_reassign_proc_fn);
        return execute();  
    } 
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
            const proc = inst_execution_fun(head(insts));
            proc(); 
            return execute();
        }
    }
    function dispatch(message) {
        return message === "start"
               ? start
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

    map(reg_name => machine("allocate_register")(reg_name), register_names);
    machine("install_operations")(ops);
    machine("install_instruction_sequence")(assemble(controller, machine));

    return machine;
}

function start(machine) {
    return machine("start")();
}

function get_register_contents(machine, register_name) {
    return get_contents(get_register(machine, register_name));
}

function set_register_contents(machine, register_name, value) {
    set_contents(get_register(machine, register_name), value);
    return "done";
}

function get_register(machine, reg_name) {
    return machine("get_register")(reg_name);
}


// ASSEMBLER

function assemble(controller, machine) {
    function receive(insts, labels) {
        update_insts(insts, labels, machine);
        return insts;
    }
    
    return extract_labels(controller, receive);
}

function extract_labels(text, receive) {
    function helper(insts, labels) { 
        const next_inst = head(text);

        return is_string(next_inst)
               ? receive(insts, pair(make_label_entry(next_inst, insts), labels))
               : receive(pair(make_inst(next_inst), insts), labels);
    }

    return is_undefined(text) || is_null(text)
           ? receive(null, null)
           : extract_labels(tail(text), helper);
}

function update_insts(insts, labels, machine) {
    const pc = get_register(machine, "pc");
    const flag = get_register(machine, "flag");
    const stack = machine("stack");
    const ops = machine("operations");

    const set_iep = set_inst_execution_fun;
    const make_ep = make_execution_function;
    return map(i => set_iep(i,
                            make_ep(inst_controller_instruction(i),
                                    labels,
                                    machine,
                                    pc,
                                    flag,
                                    stack,
                                    ops)),
               insts);
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

function make_label_entry(label_name, insts) {
    return pair(label_name, insts);
}

function lookup_label(labels, label_name) {
    const val = assoc(label_name, labels);

    return is_undefined(val)
           ? error(label_name, "undefined label -- assemble")
           : tail(val);
}

function make_execution_function(inst, labels, machine, pc, flag, stack, ops) {
    const x = head(inst);

    return x === "assign"
           ? make_assign_ef(inst, machine, labels, ops, pc)
           : x === "test"
           ? make_test_ef(inst, machine, labels, ops, flag, pc)
           : x === "branch"
           ? make_branch_ef(inst, machine, labels, flag, pc)
           : x === "go_to"
           ? make_go_to_ef(inst, machine, labels, pc)
           : x === "save"
           ? make_save_ef(inst, machine, stack, pc)
           : x === "restore"
           ? make_restore_ef(inst, machine, stack, pc)
           : x === "perform"
           ? make_perform_ef(inst, machine, labels, ops, pc)
           : x === "dump_memory" // Added to allow printing the memory vectors
           ? () => {
               display(stringify(get_register_contents(machine, "the_heads")));
               display(stringify(get_register_contents(machine, "the_tails")));
               advance_pc(pc);
           }
           : error(inst, "unknown instruction type -- assemble");
}

function make_assign_ef(inst, machine, labels, operations, pc) {
    const target = get_register(machine, assign_reg_name(inst));
    const value_exp = assign_value_exp(inst);
    const value_fun = is_operation_exp(value_exp)
          ? make_operation_exp_ef(value_exp, machine, labels, operations)
          : make_primitive_exp_ef(value_exp, machine, labels);

    function perform_assign() {
        set_contents(target, value_fun());
        advance_pc(pc); 
    }

    return perform_assign;
}

function assign_reg_name(assign_instruction) {
    return head(tail(assign_instruction));
}

function assign_value_exp(assign_instruction) { 
    return head(tail(tail(assign_instruction)));
}

function assign(reg_name, value_exp) {
    return list("assign", reg_name, value_exp);
}

function dump_memory() {
    return list("dump_memory", "the_heads", "the_tails");
}

function advance_pc(pc) {
    set_contents(pc, tail(get_contents(pc))); 
    
}

function make_test_ef(inst, machine, labels, operations, flag, pc) {
    const condition = test_condition(inst);

    if (is_operation_exp(condition)) {
    const condition_fun = make_operation_exp_ef(condition, machine,
                                             labels, operations);

        function perform_test() {
            set_contents(flag, unwrap_ptr(condition_fun()));
            advance_pc(pc); 
        }

        return perform_test; 
    } else {
        error(inst, "bad test instruction -- assemble");
    }
}

function test_condition(test_instruction) {
    return head(tail(test_instruction));
}

function test(condition) {
    return list("test", condition);
}

function make_branch_ef(inst, machine, labels, flag, pc) {
    const dest = branch_dest(inst);
    
    if (is_label_exp(dest)) {
        const insts = lookup_label(labels, label_exp_label(dest));

        function perform_branch() {
            if (get_contents(flag)) {
                set_contents(pc, insts);

            } else {
                advance_pc(pc);
            }
        }

        return perform_branch;

    } else {
        error(inst, "bad branch instruction -- assemble");
    }
}

function branch_dest(branch_instruction) {
    return head(tail(branch_instruction));
}

function branch(dest) {
    return list("branch", dest);
}

function make_goto(inst, machine, labels, pc) {
    const dest = goto_dest(inst);

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

function goto_dest(goto_instruction) {
    return head(tail(goto_instruction));
}
/*
function go_to(dest) {
    return list("go_to", dest);
}
*/
function make_save_ef(inst, machine, stack, pc) {
    const reg = get_register(machine, stack_inst_reg_name(inst));

    function perform_save() {
        push(stack, get_contents(reg));
        advance_pc(pc);
    }

    return perform_save;
}

function make_restore_ef(inst, machine, stack, pc) {
    const reg = get_register(machine, stack_inst_reg_name(inst));

    function perform_restore() {
        set_contents(reg, pop(stack));
        advance_pc(pc); 
    }

    return perform_restore;
}

function stack_inst_reg_name(stack_instruction) {
    return head(tail(stack_instruction));
}

function save(register_name) {
    return list("save", register_name);
}

function restore(register_name) {
    return list("restore", register_name);
}

function make_perform_ef(inst, machine, labels, operations, pc) {
    const action = perform_action(inst);

    if (is_operation_exp(action)) {
        const action_fun = make_operation_exp_ef(action, machine, 
                                              labels, operations);
        return () => { action_fun(); advance_pc(pc); };

    } else {
        error(inst, "bad perform instruction -- assemble");
    }
}

function perform_action(inst) {
    return head(tail(inst)); 
}

function perform(op) {
    return list("perform", op);
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

/* TODO: probably remove these -- suddenly available through new import chains
function is_register_exp(exp) {
    return is_tagged_list(exp, "reg");
}

function register_exp_reg(exp) {
    return head(tail(exp));
}

function reg(name) {
    return list("reg", name);
}

function is_constant_exp(exp) {
    return is_tagged_list(exp, "constant");
}

function constant_exp_value(exp) {
    return head(tail(exp));
}

function constant(value) {
    return list("constant", wrap_ptr(value));
}

function is_label_exp(exp) {
    return is_tagged_list(exp, "label");
}

function label_exp_label(exp) {
    return head(tail(exp));
}

function label(string) {
    return list("label", string);
}
*/

function make_operation_exp_ef(exp, machine, labels, operations) {
    const op = lookup_prim(op_exp_op(exp), operations);
    const aprocs = map(e => make_primitive_exp_ef(e, machine, labels),
                       operation_exp_operands(exp));

    function perform_operation_exp() {
        return op(map(p => p(), aprocs));
    }
    
    return perform_operation_exp;
}

/* TODO: probably remove these -- suddenly available through new import chains
function is_operation_exp(exp) {
    return is_tagged_list(head(exp), "op");
}

function operation_exp_operands(operation_exp) {
    return tail(operation_exp);
}

function op(name) {
    return list("op", name);
}
*/

function op_exp_op(operation_exp) {
    return head(tail(head(operation_exp)));
}

function lookup_prim(symbol, operations) {
    const val = assoc(symbol, operations);

    return is_undefined(val)
           ? error(symbol, "unknown operation -- assemble")
           : head(tail(val));
}

// PAIR OPERATIONS

// head in "a", tail in "b"
const pair_controller = list(
  "pair",
    save("continue"),
    assign("continue", label("pair_after_gc")),
    test(list(op("==="), reg("free"), reg("SIZE"))),
    branch(label("begin_garbage_collection")),
  "pair_after_gc",
    restore("continue"),
    perform(list(op("vector_set"), reg("the_heads"), reg("free"), reg("a"))),
    perform(list(op("vector_set"), reg("the_tails"), reg("free"), reg("b"))),
    assign("res", reg("free")),
    assign("free", list(op("inc_ptr"), reg("free"))),
    go_to(reg("continue"))
);


function underlying_javascript_closure(fn) {
    return args => apply_in_underlying_javascript(fn, args);
}

function unwrap_args(fn) {
    return args => fn(map(unwrap_ptr, args));
}

function wrap_return_value(fn) {
    return args => wrap_ptr(fn(args));
}

function primitive_function(fn) {
    return wrap_return_value(unwrap_args(underlying_javascript_closure(fn)));
}

// 5.3 MEMORY MANAGEMENT

function vector_ref(vector, idx) {
    return vector[unwrap_ptr(idx)];
}

function vector_set(vector, idx, val) {
    vector[unwrap_ptr(idx)] = val;
}

function make_vector() {
    return [];
}

function inc_ptr(ptr) {
    return make_ptr_ptr(unwrap_ptr(ptr) + 1);
}

const vector_ops = list(
    list("vector_ref", underlying_javascript_closure(vector_ref)),
    list("vector_set", underlying_javascript_closure(vector_set)),
    list("inc_ptr", underlying_javascript_closure(inc_ptr))
);

// MACHINE SETUP
const ptr_ops =
  list(
    list("make_ptr_ptr",
         underlying_javascript_closure(make_ptr_ptr)),
    list("make_null_ptr",
         underlying_javascript_closure(make_null_ptr)),
    list("make_no_value_yet_ptr",
         underlying_javascript_closure(make_no_value_yet_ptr)),
    list("make_prog_ptr", underlying_javascript_closure(make_prog_ptr)),
    list("make_broken_heart_ptr",
         underlying_javascript_closure(make_broken_heart_ptr)),
    list("is_number_ptr",
         wrap_return_value(underlying_javascript_closure(is_number_ptr))),
    list("is_bool_ptr",
         wrap_return_value(underlying_javascript_closure(is_bool_ptr))),
    list("is_string_ptr",
         wrap_return_value(underlying_javascript_closure(is_string_ptr))),
    list("is_ptr_ptr",
         wrap_return_value(underlying_javascript_closure(is_ptr_ptr))),
    list("is_null_ptr",
         wrap_return_value(underlying_javascript_closure(is_null_ptr))),
    list("is_undefined_ptr",
         wrap_return_value(underlying_javascript_closure(is_undefined_ptr))),
    list("is_prog_ptr",
         wrap_return_value(underlying_javascript_closure(is_prog_ptr))),
    list("is_no_value_yet_ptr",
         wrap_return_value(underlying_javascript_closure(is_no_value_yet_ptr))),
    list("is_broken_heart_ptr",
         wrap_return_value(underlying_javascript_closure(is_broken_heart_ptr)))
);

const primitive_ops = list(
    list("display", primitive_function(display)),
    list("error", primitive_function(error)),
    list("+", primitive_function((x, y) => x + y)),
    list("-", primitive_function((x, y) => x - y)),
    list("*", primitive_function((x, y) => x * y)),
    list("/", primitive_function((x, y) => x / y)),
    list("%", primitive_function((x, y) => x % y)),
    list("===", primitive_function((x, y) => x === y)),
    list("!==", primitive_function((x, y) => x !== y)),
    list("<", primitive_function((x, y) => x < y)),
    list("<=", primitive_function((x, y) => x <= y)),
    list(">", primitive_function((x, y) => x > y)),
    list(">=", primitive_function((x, y) => x >= y)),
    list("!", primitive_function(x => !x)),
    list("||", primitive_function((x, y) => x || y)),
    list("&&", primitive_function((x, y) => x && y))
);

const gc_ops = list(
    list("call_proc", underlying_javascript_closure(proc => proc()))
);

const gc_controller = list(
  "begin_garbage_collection",
    perform(list(op("call_proc"), reg("root_populate_proc"))),
    assign("free", list(op("make_ptr_ptr"), constant(0))),
    assign("scan", list(op("make_ptr_ptr"), constant(0))),
    assign("old", reg("root")),
    assign("relocate_continue", label("reassign_root")),
    go_to(label("relocate_old_result_in_new")),
  "reassign_root",
    assign("root", reg("new")),
    go_to(label("gc_loop")),
  "gc_loop",
    test(list(op("==="), reg("scan"), reg("free"))),
    branch(label("gc_flip")),
    assign("old", list(op("vector_ref"), reg("new_heads"), reg("scan"))),
    assign("relocate_continue", label("update_head")),
    go_to(label("relocate_old_result_in_new")),
  "update_head",
    perform(list(op("vector_set"), reg("new_heads"), reg("scan"), reg("new"))),
    assign("old", list(op("vector_ref"), reg("new_tails"), reg("scan"))),
    assign("relocate_continue", label("update_tail")),
    go_to(label("relocate_old_result_in_new")),
  "update_tail",
    perform(list(op("vector_set"), reg("new_tails"), reg("scan"), reg("new"))),
    assign("scan", list(op("inc_ptr"), reg("scan"))),
    go_to(label("gc_loop")),
  "relocate_old_result_in_new",
    test(list(op("is_ptr_ptr"), reg("old"))),
    branch(label("gc_pair")),
    assign("new", reg("old")),
    go_to(reg("relocate_continue")),
  "gc_pair",
    assign("oldht", list(op("vector_ref"), reg("the_heads"), reg("old"))),
    test(list(op("is_broken_heart_ptr"), reg("oldht"))),
    branch(label("already_moved")),
    assign("new", reg("free")),
    // new location for pair
    // Update free pointer
    assign("free", list(op("inc_ptr"), reg("free"))),
    // Copy the head and tail to new memory
    perform(list(op("vector_set"),
                 reg("new_heads"), reg("new"), reg("oldht"))),
    assign("oldht", list(op("vector_ref"), reg("the_tails"), reg("old"))),
    perform(list(op("vector_set"),
                 reg("new_tails"), reg("new"), reg("oldht"))),
    // Construct the broken heart
    assign("oldht", list(op("make_broken_heart_ptr"))),
    perform(list(op("vector_set"),
                 reg("the_heads"), reg("old"), reg("oldht"))),
    perform(list(op("vector_set"),
                 reg("the_tails"), reg("old"), reg("new"))),
    go_to(reg("relocate_continue")),
  "already_moved",
    assign("new", list(op("vector_ref"), reg("the_tails"), reg("old"))),
    go_to(reg("relocate_continue")),
  "gc_flip",
    perform(list(op("call_proc"), reg("stack_reassign_proc"))),
    assign("temp", reg("the_tails")),
    assign("the_tails", reg("new_tails")),
    assign("new_tails", reg("temp")),
    assign("temp", reg("the_heads")),
    assign("the_heads", reg("new_heads")),
    assign("new_heads", reg("temp")),
    perform(list(op("call_proc"), reg("root_restore_proc"))),
    go_to(reg("continue"))
);

const error_controller = list(
    "error",
    perform(list(op("error"), reg("res"), reg("err"))),
    go_to(label("end_evaluation"))
);

const begin_controller =  list(
  "fig_5_14",
  "pair4",
    assign("a", constant(4)),
    assign("b", list(op("make_null_ptr"))),
    assign("continue", label("garbage1")),
    go_to(label("pair")),
    /// The following creates a garbage
    /// pair (9999, 9999) which will
    /// not affect live object count at
    /// the end of the program. You can
    /// verify by adding more garbage
    /// or remove this line. Or
    /// uncomment the use of
    /// dump_memory() below, before and
    /// after GC.
  "garbage1",
    assign("a", constant(9999)),
    assign("b", constant(9999)),
    assign("continue", label("pair2")),
    go_to(label("pair")),
  "pair2",
    assign("a", constant(3)),
    assign("b", reg("res")),
    assign("continue", label("garbage2")),
    go_to(label("pair")), 
    /// The following creates a garbage
    /// pair (9999, 9999) which will
    /// not affect live object count at
    /// the end of the program. You can
    /// verify by adding more garbage
    /// or remove this line. Or
    /// uncomment the use of
    /// dump_memory() below, before and
    /// after GC.
  "garbage2",
    assign("a", constant(9999)),
    assign("b", constant(9999)),
    assign("continue", label("pair7")),
    go_to(label("pair")),
  "pair7",
    assign("temp", reg("res")),
    assign("a", constant(2)),
    assign("b", list(op("make_null_ptr"))),
    assign("continue", label("pair5")),
    go_to(label("pair")),
  "pair5",
    assign("a", constant(1)),
    assign("b", reg("res")),
    assign("continue", label("pair1")),
    go_to(label("pair")),
  "pair1",
    assign("a", reg("res")),
    assign("b", reg("temp")),
    assign("continue", label("done")),
    go_to(label("pair")),
  "done",
    // dump_memory(), // uncomment to get a dump of heads and tails vectors
    assign("continue", label("after_gc")),
    go_to(label("begin_garbage_collection")),
  "after_gc",
    // dump_memory(), // uncomment to get a dump of heads and tails vectors
    go_to(label("end_evaluation")));

const end_controller = list(
    "end_evaluation"
);

const ops = accumulate(append, null, list(
    vector_ops,
    ptr_ops,
    gc_ops,
    primitive_ops
));

const controller = accumulate(append, null, list(
    begin_controller,
    pair_controller,
    gc_controller,
    error_controller,
    end_controller
));

function make_evaluator_machine(size) {
    const evaluator_machine = make_machine(null, ops, controller);
    set_register_contents(evaluator_machine, "SIZE", wrap_ptr(size));
    return evaluator_machine;
}

const evaluator_machine = make_evaluator_machine(10000);

set_register_contents(evaluator_machine, "a", wrap_ptr(206));
set_register_contents(evaluator_machine, "b", wrap_ptr(40));

start(evaluator_machine);
get_register_contents(evaluator_machine, "free");
// [ 'ptr', 108 ] The number of live objects in the program at termination

// expected: [ 'ptr', 108 ]
