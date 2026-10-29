// provided by GitHub user devinryu

function entry(tree) { return head(tree); }
function left_branch(tree) { return head(tail(tree)); }
function right_branch(tree) { return head(tail(tail(tree))); }
function make_tree(entry, left, right) { 
    return list(entry, left, right);
}

// kv is list(key, value)
function adjoin_set(kv, set) {
    return is_null(set)
           ? make_tree(kv, null, null)
           : head(kv) === head(entry(set))
           ? set
           : head(kv) < head(entry(set))
           ? make_tree(entry(set),
                       adjoin_set(kv, left_branch(set)),
                       right_branch(set))
           : make_tree(entry(set),
                       left_branch(set),
                       adjoin_set(kv, right_branch(set)));
}

function make_table() {
    let local_table = null;
    function lookup(given_key, tree_of_records) {
        if (is_null(tree_of_records)) {
            return null;
        } else {
            const this_entry = entry(tree_of_records);
            const this_key = head(this_entry);
            return given_key === this_key 
                   ? this_entry
                   : given_key < this_key
                   ? lookup(given_key, 
                            left_branch(tree_of_records))
                   : lookup(given_key, 
                            right_branch(tree_of_records));
        }
    }
    function insert(k, v) {
        let record = lookup(k, local_table);
        if (is_null(record)) {
            local_table = adjoin_set(list(k, v), local_table);
        } else {
            set_tail(record, list(v));
        }
    }
    function get(k) {
        return head(tail(lookup(k, local_table)));
    }
    function print() {
        return display(local_table);
    }
    function dispatch(m) {
        return m === "lookup"
        ? get
        : m === "insert"
        ? insert
        : m  === "print"
        ? print
        : error(m, "error");
    }
    return dispatch;
}

const t = make_table();
const get = t("lookup");
const put = t("insert");
const print = t("print");

// The test results

put(3, "d");
put(1, "a");
put(2, "b");
put(2, "c");
put(4, "e");
put(5, "f");

print();

display(get(2)); // displays: "c"
