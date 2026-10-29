let coercion_list = null;

function clear_coercion_list() {
    coercion_list = null;
}

function put_coercion(type1, type2, item) {
    if (is_null(get_coercion(type1, type2))) {
        coercion_list = pair(list(type1, type2, item),
                             coercion_list);
    } else {
        return coercion_list;
    }
}

function get_coercion(type1, type2) {
    function get_type1(list_item) {
        return head(list_item);
    }
    function get_type2(list_item) {
        return head(tail(list_item));
    }
    function get_item(list_item) {
        return head(tail(tail(list_item)));
    }
    function get_coercion_iter(items) {
        if (is_null(items)) {
            return undefined;
        } else {
            const top = head(items);
            return equal(type1, get_type1(top)) &&
                   equal(type2, get_type2(top))
                   ? get_item(top)
                   : get_coercion_iter(tail(items));
        }
    }
    return get_coercion_iter(coercion_list);
}
