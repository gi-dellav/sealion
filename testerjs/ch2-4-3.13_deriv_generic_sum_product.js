// chapter=4 
function is_variable(x) { return is_string(x); }
function is_same_variable(v1, v2) {
    return is_variable(v1) && is_variable(v2) && v1 === v2;
}
function deriv(exp, variable) {
    return is_number(exp)
           ? 0
           : is_variable(exp)
           ? is_same_variable(exp, variable) ? 1 : 0
           : get("deriv", operator(exp))(operands(exp), variable);
}
function operator(exp) { return head(exp); }

function operands(exp) { return tail(exp); }
function make_sum(a1, a2) {
    return list("+", a1, a2);
}
            
function make_product(m1, m2) {
    return list("*", m1, m2);
}
            
function addend(operands) {
    return head(operands);
}
            
function augend(operands) {
    return head(tail(operands));
}
            
function multiplier(operands) {
    return head(operands);
}
            
function multiplicand(operands) {
    return head(tail(operands));
}
// operation_table, put and get
// from chapter 3 (section 3.3.3)
function assoc(key, records) {
    return is_null(records)
           ? undefined
           : equal(key, head(head(records)))
           ? head(records)
           : assoc(key, tail(records));
}
function make_table() {
    const local_table = list("*table*");
    function lookup(key_1, key_2) {
        const subtable = assoc(key_1, tail(local_table));
        if (is_undefined(subtable)) {
            return undefined;
        } else {
            const record = assoc(key_2, tail(subtable));
            return is_undefined(record)
                   ? undefined
                   : tail(record);
        }
    }
    function insert(key_1, key_2, value) {
        const subtable = assoc(key_1, tail(local_table));
        if (is_undefined(subtable)) {
            set_tail(local_table,
                     pair(list(key_1, pair(key_2, value)),
                          tail(local_table)));
        } else {
            const record = assoc(key_2, tail(subtable));
            if (is_undefined(record)) {
                set_tail(subtable,
                         pair(pair(key_2, value), tail(subtable)));
            } else {
                set_tail(record, value);
            }
        }
    }
    function dispatch(m) {
        return m === "lookup"
               ? lookup
               : m === "insert"
               ? insert
               : error(m, "unknown operation -- table");
    }
    return dispatch;
}
const operation_table = make_table();
const get = operation_table("lookup");
const put = operation_table("insert");
function deriv_sum(operands, variable) {		
    return make_sum(deriv(addend(operands), variable),
                    deriv(augend(operands), variable));
}
function deriv_product(operands, variable) {
    return make_sum(make_product(multiplier(operands),
                        deriv(multiplicand(operands),
                              variable)),
                    make_product(deriv(multiplier(
                                             operands),
                                       variable),
                        multiplicand(operands)));
}
function install_deriv() {
    put("deriv", "+", deriv_sum);
    put("deriv", "*", deriv_product);
    return "done";
}
install_deriv();

head(tail(head(tail(deriv(list("*", list("*", "x", "y"), list("+", "x", 3)), "x")))));

// expected: [ '*', [ 'x', [ 'y', null ] ] ]
