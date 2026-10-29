// chapter=3 
function square(x) {
    return x * x;
}
function make_from_real_imag(x, y) {
    function dispatch(op) {
        return op === "real_part"
               ? x
               : op === "imag_part"
               ? y
               : op === "magnitude"
               ? math_sqrt(square(x) + square(y))
               : op === "angle"
               ? math_atan2(y, x)
               : error(op, "unknown op -- make_from_real_imag");
    }
    return dispatch;
}
    
function make_from_mag_ang(r, a) {
    function dispatch(op) {
        return op === "real_part"
               ? r * math_cos(a)
               : op === "imag_part"
               ? r * math_sin(a)
               : op === "magnitude"
               ? r
               : op === "angle"
               ? a
               : error(op, "unknown op -- make_from_real_imag");
    }
    return dispatch;
}
    
function apply_generic(op, arg) {
    return head(arg)(op);
}
function real_part(z) {
   return apply_generic("real_part", list(z));
}
function imag_part(z) {
   return apply_generic("imag_part", list(z));
}
function magnitude(z) {
   return apply_generic("magnitude", list(z));
}
function angle(z) {
   return apply_generic("angle", list(z));
}
function add_complex(z1, z2) {
    return make_from_real_imag(
               real_part(z1) + real_part(z2),
               imag_part(z1) + imag_part(z2));
}
function sub_complex(z1, z2) {
    return make_from_real_imag(
               real_part(z1) - real_part(z2),
               imag_part(z1) - imag_part(z2));
}
function mul_complex(z1, z2) {
    return make_from_mag_ang(
               magnitude(z1) * magnitude(z2),
               angle(z1) + angle(z2));
}
function div_complex(z1, z2) {
    return make_from_mag_ang(
               magnitude(z1) / magnitude(z2),
               angle(z1) - angle(z2));
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
	                       pair(pair(key_2, value),
                              tail(subtable)));
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
               : "undefined operation -- table";
    }
    return dispatch;
}
const operation_table = make_table();
const get = operation_table("lookup");
const put = operation_table("insert");
function attach_tag(type_tag, contents) {
    return pair(type_tag, contents);
}
function type_tag(datum) {
    return is_pair(datum)
           ? head(datum)
           : error(datum, "bad tagged datum -- type_tag");
}
function contents(datum) {
    return is_pair(datum)
           ? tail(datum)
           : error(datum, "bad tagged datum -- contents");
}
function install_rectangular_package() {
    function real_part(z) { return head(z); }
    function imag_part(z) { return tail(z); }
    function make_from_real_imag(x, y) { return pair(x, y); }
    function magnitude(z) {
        return math_sqrt(square(real_part(z)) +
                   square(imag_part(z)));
    }
    function angle(z) {
        return math_atan2(imag_part(z), real_part(z));
    }
    function make_from_mag_ang(r, a) {
        return pair(r * math_cos(a), r * math_sin(a));
    }
    // interface to the rest of the system
    function tag(x) {
        return attach_tag("rectangular", x);
    }
    put("real_part", list("rectangular"), real_part);
    put("imag_part", list("rectangular"), imag_part);
    put("magnitude", list("rectangular"), magnitude);
    put("angle", list("rectangular"), angle);
    put("make_from_real_imag", "rectangular",
        (x, y) => tag(make_from_real_imag(x, y)));
    put("make_from_mag_ang", "rectangular",
        (r, a) => tag(make_from_mag_ang(r, a)));
    return "done";
}

install_rectangular_package();
function install_polar_package() {
    // internal functions
    function magnitude(z) { return head(z); }
    function angle(z) { return tail(z); }
    function make_from_mag_ang(r, a) { return pair(r, a); }
    function real_part(z) {
       return magnitude(z) * math_cos(angle(z));
    }
    function imag_part(z) {
       return magnitude(z) * math_sin(angle(z));
    }
    function make_from_real_imag(x, y) {
       return pair(math_sqrt(square(x) + square(y)),
                   math_atan2(y, x));
    }

    // interface to the rest of the system
    function tag(x) { return attach_tag("polar", x); }
    put("real_part", list("polar"), real_part);
    put("imag_part", list("polar"), imag_part);
    put("magnitude", list("polar"), magnitude);
    put("angle", list("polar"), angle);
    put("make_from_real_imag", "polar", 
        (x, y) => tag(make_from_real_imag(x, y)));
    put("make_from_mag_ang", "polar",
        (r, a) => tag(make_from_mag_ang(r, a)));
    return "done";
}

install_polar_package();


const my_complex_number = 
    make_from_mag_ang(3.0, 0.7);

const result = 
    add_complex(my_complex_number,
                my_complex_number);

real_part(result);

// expected: 4.589053123706931
