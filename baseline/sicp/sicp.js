import { parse as acorn_parse, tokenizer as acorn_tokenizer } from './vendor/acorn.mjs';

const InfinityValue = Infinity;
const NaNValue = NaN;
const undefinedValue = undefined;

const math_E = Math.E;
const math_LN10 = Math.LN10;
const math_LN2 = Math.LN2;
const math_LOG10E = Math.LOG10E;
const math_LOG2E = Math.LOG2E;
const math_PI = Math.PI;
const math_SQRT1_2 = Math.SQRT1_2;
const math_SQRT2 = Math.SQRT2;

const math_abs = Math.abs;
const math_acos = Math.acos;
const math_acosh = Math.acosh;
const math_asin = Math.asin;
const math_asinh = Math.asinh;
const math_atan = Math.atan;
const math_atan2 = Math.atan2;
const math_atanh = Math.atanh;
const math_cbrt = Math.cbrt;
const math_ceil = Math.ceil;
const math_clz32 = Math.clz32;
const math_cos = Math.cos;
const math_cosh = Math.cosh;
const math_exp = Math.exp;
const math_expm1 = Math.expm1;
const math_floor = Math.floor;
const math_fround = Math.fround;
const math_hypot = Math.hypot;
const math_imul = Math.imul;
const math_log = Math.log;
const math_log10 = Math.log10;
const math_log1p = Math.log1p;
const math_log2 = Math.log2;
const math_max = Math.max;
const math_min = Math.min;
const math_pow = Math.pow;
const math_random = Math.random;
const math_round = Math.round;
const math_sign = Math.sign;
const math_sin = Math.sin;
const math_sinh = Math.sinh;
const math_sqrt = Math.sqrt;
const math_tan = Math.tan;
const math_tanh = Math.tanh;
const math_trunc = Math.trunc;

function pair(x, y) {
    return [x, y];
}

function is_pair(x) {
    return Array.isArray(x) && x.length === 2;
}

function head(p) {
    if (is_pair(p)) {
        return p[0];
    }
    throw new Error('head(xs) expects a pair as argument xs, but encountered ' + stringify(p));
}

function tail(p) {
    if (is_pair(p)) {
        return p[1];
    }
    throw new Error('tail(xs) expects a pair as argument xs, but encountered ' + stringify(p));
}

function is_null(x) {
    return x === null;
}

function list(...elements) {
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function is_list(xs) {
    let current = xs;
    while (is_pair(current)) {
        current = tail(current);
    }
    return is_null(current);
}

function set_head(p, x) {
    if (is_pair(p)) {
        p[0] = x;
        return undefinedValue;
    }
    throw new Error('set_head(xs,x) expects a pair as argument xs, but encountered ' + stringify(p));
}

function set_tail(p, x) {
    if (is_pair(p)) {
        p[1] = x;
        return undefinedValue;
    }
    throw new Error('set_tail(xs,x) expects a pair as argument xs, but encountered ' + stringify(p));
}

function array_length(x) {
    return x.length;
}

function is_array(x) {
    return Array.isArray(x);
}

function is_number(v) {
    return typeof v === 'number';
}

function is_string(v) {
    return typeof v === 'string';
}

function is_function(v) {
    return typeof v === 'function';
}

function is_boolean(v) {
    return typeof v === 'boolean';
}

function is_undefined(v) {
    return typeof v === 'undefined';
}

function arity(f) {
    if (typeof f === 'function') {
        return f.length;
    }
    throw new Error('arity expects a function as argument');
}

function get_time() {
    return Date.now();
}

function parse_int(s, i) {
    if (typeof s === 'string' &&
        typeof i === 'number' &&
        Number.isInteger(i) &&
        i >= 2 &&
        i <= 36) {
        return parseInt(s, i);
    }
    throw new Error('parse_int expects two arguments a string s, and a positive integer i between 2 and 36, inclusive.');
}

function char_at(s, i) {
    if (typeof s !== 'string') {
        throw new Error('char_at expects the first argument to be a string.');
    }
    if (typeof i !== 'number' || !Number.isInteger(i) || i < 0) {
        throw new Error('char_at expects the second argument to be a nonnegative integer.');
    }
    return s[i];
}

function apply_in_underlying_javascript(f, xs) {
    const args = [];
    let current = xs;
    while (is_pair(current)) {
        args.push(head(current));
        current = tail(current);
    }
    return f.apply(f, args);
}

function __access_named_export__(named_exports, lookup_name) {
    let current = named_exports;
    while (is_pair(current)) {
        const entry = head(current);
        if (is_pair(entry) && head(entry) === lookup_name) {
            return tail(entry);
        }
        current = tail(current);
    }
    return undefinedValue;
}

function __access_export__(exports, lookup_name) {
    return lookup_name === 'default'
        ? head(exports)
        : __access_named_export__(tail(exports), lookup_name);
}

function equal(xs, ys) {
    return is_pair(xs)
        ? (is_pair(ys) &&
           equal(head(xs), head(ys)) &&
           equal(tail(xs), tail(ys)))
        : is_null(xs)
        ? is_null(ys)
        : is_number(xs)
        ? (is_number(ys) && xs === ys)
        : is_boolean(xs)
        ? (is_boolean(ys) && ((xs && ys) || (!xs && !ys)))
        : is_string(xs)
        ? (is_string(ys) && xs === ys)
        : is_undefined(xs)
        ? is_undefined(ys)
        : is_function(xs)
        ? (is_function(ys) && xs === ys)
        : false;
}

function length(xs) {
    let result = 0;
    let current = xs;
    while (is_pair(current)) {
        result = result + 1;
        current = tail(current);
    }
    return result;
}

function map(f, xs) {
    const elements = [];
    let current = xs;
    while (is_pair(current)) {
        elements.push(f(head(current)));
        current = tail(current);
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function build_list(f, n) {
    const elements = [];
    for (let i = 0; i < n; i = i + 1) {
        elements.push(f(i));
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function for_each(f, xs) {
    let current = xs;
    while (is_pair(current)) {
        f(head(current));
        current = tail(current);
    }
    return true;
}

function stringify(value, indent = 2, splitlineThreshold = 80) {
    const ancestors = new Set();
    function convert(v) {
        if (v === null) {
            return 'null';
        }
        if (v === undefinedValue) {
            return 'undefined';
        }
        if (typeof v === 'string') {
            return JSON.stringify(v);
        }
        if (typeof v === 'function') {
            return v.toString();
        }
        if (typeof v !== 'object') {
            return String(v);
        }
        if (ancestors.has(v)) {
            return '...<circular>';
        }
        ancestors.add(v);
        let result;
        if (Array.isArray(v)) {
            result = '[' + v.map(convert).join(', ') + ']';
        } else {
            result = String(v);
        }
        ancestors.delete(v);
        return result;
    }
    return convert(value);
}

function list_to_string(xs) {
    if (is_null(xs)) {
        return 'null';
    }
    if (is_pair(xs)) {
        return '[' + list_to_string(head(xs)) + ',' + list_to_string(tail(xs)) + ']';
    }
    return stringify(xs);
}

function list_repr(xs) {
    if (is_null(xs)) {
        return 'null';
    }
    if (!is_pair(xs)) {
        return stringify(xs);
    }
    const elements = [];
    let current = xs;
    while (is_pair(current)) {
        elements.push(head(current));
        current = tail(current);
    }
    if (is_null(current)) {
        return 'list(' + elements.map(element =>
            is_list(element) ? list_repr(element) : stringify(element)).join(', ') + ')';
    }
    return stringify(xs);
}

function display(v, s) {
    if (s !== undefinedValue && typeof s !== 'string') {
        throw new TypeError('display expects the second argument to be a string');
    }
    console.log((s === undefinedValue ? '' : s + ' ') + stringify(v));
    return v;
}

function display_list(xs, s) {
    if (s !== undefinedValue && typeof s !== 'string') {
        throw new TypeError('display_list expects the second argument to be a string');
    }
    console.log((s === undefinedValue ? '' : s + ' ') + list_repr(xs));
    return xs;
}

function draw_data(...values) {
    return values[0];
}

function error(v, s) {
    throw new Error((s === undefinedValue ? '' : s + ' ') + stringify(v));
}

function prompt(s) {
    return null;
}

function reverse(xs) {
    let result = null;
    let current = xs;
    while (is_pair(current)) {
        result = pair(head(current), result);
        current = tail(current);
    }
    return result;
}

function append(xs, ys) {
    const elements = [];
    let current = xs;
    while (is_pair(current)) {
        elements.push(head(current));
        current = tail(current);
    }
    let result = ys;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function member(v, xs) {
    let current = xs;
    while (is_pair(current)) {
        if (head(current) === v) {
            return current;
        }
        current = tail(current);
    }
    return null;
}

function remove(v, xs) {
    const prefix = [];
    let current = xs;
    while (is_pair(current)) {
        if (head(current) === v) {
            let result = tail(current);
            for (let i = prefix.length - 1; i >= 0; i = i - 1) {
                result = pair(prefix[i], result);
            }
            return result;
        }
        prefix.push(head(current));
        current = tail(current);
    }
    return xs;
}

function remove_all(v, xs) {
    const elements = [];
    let changed = false;
    let current = xs;
    while (is_pair(current)) {
        if (head(current) === v) {
            changed = true;
        } else {
            elements.push(head(current));
        }
        current = tail(current);
    }
    if (!changed) {
        return xs;
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function filter(pred, xs) {
    const elements = [];
    let current = xs;
    while (is_pair(current)) {
        if (pred(head(current))) {
            elements.push(head(current));
        }
        current = tail(current);
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function enum_list(start, end) {
    const elements = [];
    for (let i = start; i <= end; i = i + 1) {
        elements.push(i);
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function list_ref(xs, n) {
    let current = xs;
    let index = n;
    while (index > 0) {
        current = tail(current);
        index = index - 1;
    }
    return head(current);
}

function accumulate(f, initial, xs) {
    const elements = [];
    let current = xs;
    while (is_pair(current)) {
        elements.push(head(current));
        current = tail(current);
    }
    let result = initial;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = f(elements[i], result);
    }
    return result;
}

function stream_tail(xs) {
    if (!is_pair(xs)) {
        throw new Error('stream_tail(xs) expects a pair as argument xs, but encountered ' + stringify(xs));
    }
    const theTail = tail(xs);
    if (typeof theTail !== 'function') {
        throw new Error('stream_tail(xs) expects a function as the tail of the argument pair xs, but encountered ' + stringify(theTail));
    }
    return theTail();
}

function stream(...elements) {
    return list_to_stream(list(...elements));
}

function list_to_stream(xs) {
    return is_null(xs)
        ? null
        : pair(head(xs), () => list_to_stream(tail(xs)));
}

function is_stream(xs) {
    let current = xs;
    while (is_pair(current)) {
        const next = tail(current);
        if (typeof next !== 'function' || next.length !== 0) {
            return false;
        }
        current = next();
    }
    return is_null(current);
}

function stream_to_list(xs) {
    const elements = [];
    let current = xs;
    while (!is_null(current)) {
        elements.push(head(current));
        current = stream_tail(current);
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function stream_length(xs) {
    let result = 0;
    let current = xs;
    while (!is_null(current)) {
        result = result + 1;
        current = stream_tail(current);
    }
    return result;
}

function stream_map(f, s) {
    return is_null(s)
        ? null
        : pair(f(head(s)), () => stream_map(f, stream_tail(s)));
}

function build_stream(f, n) {
    function build(i) {
        return i >= n
            ? null
            : pair(f(i), () => build(i + 1));
    }
    return build(0);
}

function stream_for_each(f, xs) {
    let current = xs;
    while (!is_null(current)) {
        f(head(current));
        current = stream_tail(current);
    }
    return true;
}

function stream_reverse(xs) {
    const elements = [];
    let current = xs;
    while (!is_null(current)) {
        elements.push(head(current));
        current = stream_tail(current);
    }
    elements.reverse();
    return list_to_stream(list(...elements));
}

function stream_append(xs, ys) {
    return is_null(xs)
        ? ys
        : pair(head(xs), () => stream_append(stream_tail(xs), ys));
}

function stream_member(v, xs) {
    let current = xs;
    while (!is_null(current)) {
        if (head(current) === v) {
            return current;
        }
        current = stream_tail(current);
    }
    return null;
}

function stream_remove(v, xs) {
    return is_null(xs)
        ? null
        : head(xs) === v
        ? stream_tail(xs)
        : pair(head(xs), () => stream_remove(v, stream_tail(xs)));
}

function stream_remove_all(v, xs) {
    return is_null(xs)
        ? null
        : head(xs) === v
        ? stream_remove_all(v, stream_tail(xs))
        : pair(head(xs), () => stream_remove_all(v, stream_tail(xs)));
}

function stream_filter(pred, xs) {
    return is_null(xs)
        ? null
        : pred(head(xs))
        ? pair(head(xs), () => stream_filter(pred, stream_tail(xs)))
        : stream_filter(pred, stream_tail(xs));
}

function enum_stream(start, end) {
    return start > end
        ? null
        : pair(start, () => enum_stream(start + 1, end));
}

function integers_from(n) {
    return pair(n, () => integers_from(n + 1));
}

function eval_stream(s, n) {
    const elements = [];
    let current = s;
    for (let i = 0; i < n; i = i + 1) {
        elements.push(head(current));
        current = stream_tail(current);
    }
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function stream_ref(s, n) {
    let current = s;
    let index = n;
    while (index > 0) {
        current = stream_tail(current);
        index = index - 1;
    }
    return head(current);
}

const pending_timeouts = new Set();

function set_timeout(f, t) {
    const id = setTimeout(() => {
        pending_timeouts.delete(id);
        f();
    }, t);
    pending_timeouts.add(id);
    return undefinedValue;
}

function clear_all_timeout() {
    for (const id of pending_timeouts) {
        clearTimeout(id);
    }
    pending_timeouts.clear();
    return undefinedValue;
}

function vector_to_list(elements) {
    let result = null;
    for (let i = elements.length - 1; i >= 0; i = i - 1) {
        result = pair(elements[i], result);
    }
    return result;
}

function has_toplevel_declaration(statements) {
    return statements.some(node =>
        node.type === 'VariableDeclaration' || node.type === 'FunctionDeclaration');
}

function transform_program_body(statements) {
    statements = statements.filter(statement => statement.type !== 'EmptyStatement');
    if (statements.length === 1) {
        return transform_node(statements[0]);
    }
    return list('sequence', vector_to_list(statements.map(transform_node)));
}

function transform_block_body(statements) {
    statements = statements.filter(statement => statement.type !== 'EmptyStatement');
    return has_toplevel_declaration(statements)
        ? list('block', transform_program_body(statements))
        : transform_program_body(statements);
}

function transform_node(node) {
    switch (node.type) {
        case 'Program':
            return transform_program_body(node.body);
        case 'BlockStatement':
            return transform_block_body(node.body);
        case 'ExpressionStatement':
            return transform_node(node.expression);
        case 'EmptyStatement':
            return list('sequence', null);
        case 'IfStatement':
            return list('conditional_statement',
                transform_node(node.test),
                transform_node(node.consequent),
                node.alternate === null
                    ? transform_program_body([])
                    : transform_node(node.alternate));
        case 'FunctionDeclaration':
            return list('function_declaration',
                transform_node(node.id),
                vector_to_list(node.params.map(transform_node)),
                transform_block_body(node.body.body));
        case 'FunctionExpression':
            return list('lambda_expression',
                vector_to_list(node.params.map(transform_node)),
                transform_block_body(node.body.body));
        case 'VariableDeclaration': {
            const declaration = node.declarations[0];
            return list(node.kind === 'const'
                ? 'constant_declaration'
                : 'variable_declaration',
                transform_node(declaration.id),
                transform_node(declaration.init));
        }
        case 'ReturnStatement':
            return list('return_statement', transform_node(node.argument));
        case 'CallExpression':
            return list('application',
                transform_node(node.callee),
                vector_to_list(node.arguments.map(transform_node)));
        case 'NewExpression':
            return list('new_expression',
                transform_node(node.callee),
                vector_to_list(node.arguments.map(transform_node)));
        case 'UnaryExpression':
            return list('unary_operator_combination',
                node.operator === '-' ? '-unary' : node.operator,
                transform_node(node.argument));
        case 'BinaryExpression':
            return list('binary_operator_combination',
                node.operator,
                transform_node(node.left),
                transform_node(node.right));
        case 'LogicalExpression':
            return list('logical_composition',
                node.operator,
                transform_node(node.left),
                transform_node(node.right));
        case 'ConditionalExpression':
            return list('conditional_expression',
                transform_node(node.test),
                transform_node(node.consequent),
                transform_node(node.alternate));
        case 'ArrowFunctionExpression':
            return list('lambda_expression',
                vector_to_list(node.params.map(transform_node)),
                node.body.type === 'BlockStatement'
                    ? transform_block_body(node.body.body)
                    : list('return_statement', transform_node(node.body)));
        case 'Identifier':
            return list('name', node.name);
        case 'Literal':
            return list('literal', node.value);
        case 'ArrayExpression':
            return list('array_expression',
                vector_to_list(node.elements.map(transform_node)));
        case 'AssignmentExpression':
            return list(node.left.type === 'MemberExpression'
                ? 'object_assignment'
                : 'assignment',
                transform_node(node.left),
                transform_node(node.right));
        case 'ForStatement':
            return list('for_loop',
                transform_node(node.init),
                transform_node(node.test),
                transform_node(node.update),
                transform_node(node.body));
        case 'WhileStatement':
            return list('while_loop',
                transform_node(node.test),
                transform_node(node.body));
        case 'BreakStatement':
            return list('break_statement');
        case 'ContinueStatement':
            return list('continue_statement');
        case 'ObjectExpression':
            return list('object_expression',
                vector_to_list(node.properties.map(transform_node)));
        case 'Property':
            return list('key_value_pair',
                node.key.type === 'Identifier'
                    ? list('property', node.key.name)
                    : transform_node(node.key),
                transform_node(node.value));
        case 'MemberExpression':
            return list('object_access',
                transform_node(node.object),
                !node.computed && node.property.type === 'Identifier'
                    ? list('property', node.property.name)
                    : transform_node(node.property));
        case 'ClassDeclaration':
            return list('class_declaration',
                list('name',
                    node.id === null ? null : node.id.name,
                    node.superClass === null ? null : transform_node(node.superClass),
                    vector_to_list(node.body.body.map(transform_node))));
        case 'MethodDefinition':
            return list('method_definition',
                node.kind,
                node.static,
                transform_node(node.key),
                transform_node(node.value));
        case 'ThisExpression':
            return list('this_expression');
        case 'Super':
            return list('super_expression');
        case 'TryStatement':
            return list('try_statement',
                transform_node(node.block),
                node.handler === null
                    ? null
                    : list('name', node.handler.param.name),
                node.handler === null ? null : transform_node(node.handler.body));
        case 'ThrowStatement':
            return list('throw_statement', transform_node(node.argument));
        case 'SpreadElement':
            return list('spread_element', transform_node(node.argument));
        case 'RestElement':
            return list('rest_element', transform_node(node.argument));
        default:
            throw new Error('Cannot transform unknown type: ' + node.type);
    }
}

function parse(x) {
    const program = acorn_parse(x, {
        ecmaVersion: 'latest',
        sourceType: 'script'
    });
    return transform_node(program);
}

function tokenize(x) {
    const tokens = [];
    const tokenizer = acorn_tokenizer(x, { ecmaVersion: 'latest' });
    let token = tokenizer.getToken();
    while (token.type.label !== 'eof') {
        tokens.push(x.substring(token.start, token.end));
        token = tokenizer.getToken();
    }
    return vector_to_list(tokens);
}

export {
    InfinityValue as Infinity,
    NaNValue as NaN,
    undefinedValue as undefined,
    math_E,
    math_LN10,
    math_LN2,
    math_LOG10E,
    math_LOG2E,
    math_PI,
    math_SQRT1_2,
    math_SQRT2,
    math_abs,
    math_acos,
    math_acosh,
    math_asin,
    math_asinh,
    math_atan,
    math_atan2,
    math_atanh,
    math_cbrt,
    math_ceil,
    math_clz32,
    math_cos,
    math_cosh,
    math_exp,
    math_expm1,
    math_floor,
    math_fround,
    math_hypot,
    math_imul,
    math_log,
    math_log10,
    math_log1p,
    math_log2,
    math_max,
    math_min,
    math_pow,
    math_random,
    math_round,
    math_sign,
    math_sin,
    math_sinh,
    math_sqrt,
    math_tan,
    math_tanh,
    math_trunc,
    __access_export__,
    __access_named_export__,
    accumulate,
    append,
    apply_in_underlying_javascript,
    arity,
    array_length,
    build_list,
    build_stream,
    char_at,
    clear_all_timeout,
    display,
    display_list,
    draw_data,
    enum_list,
    enum_stream,
    equal,
    error,
    eval_stream,
    filter,
    for_each,
    get_time,
    head,
    integers_from,
    is_array,
    is_boolean,
    is_function,
    is_list,
    is_null,
    is_number,
    is_pair,
    is_stream,
    is_string,
    is_undefined,
    length,
    list,
    list_ref,
    list_to_stream,
    list_to_string,
    map,
    member,
    pair,
    parse,
    parse_int,
    prompt,
    remove,
    remove_all,
    reverse,
    set_head,
    set_tail,
    set_timeout,
    stream,
    stream_append,
    stream_filter,
    stream_for_each,
    stream_length,
    stream_map,
    stream_member,
    stream_ref,
    stream_remove,
    stream_remove_all,
    stream_reverse,
    stream_tail,
    stream_to_list,
    stringify,
    tail,
    tokenize
};
