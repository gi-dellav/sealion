function is_truthy(x) { return ! is_falsy(x); }

function is_falsy(x) {
    return (is_boolean(x) && !x )                  ||
           (is_number(x) && (x === 0 || x !== x )) ||
           (is_string(x) && x === "")              ||
           is_null(x)                              ||
           is_undefined(x);
}
