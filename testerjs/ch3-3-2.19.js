// provided by GitHub user devinryu	  
	  
function make_queue() {
    let front_ptr = null;
    let rear_ptr = null;
    function is_empty_queue() {
        return is_null(front_ptr);
    }
    function insert_queue(item) {
        let new_pair = pair(item, null);
        if (is_empty_queue()) {
            front_ptr = new_pair;
            rear_ptr = new_pair;
        } else {
            set_tail(rear_ptr, new_pair);
            rear_ptr = new_pair;
        }
    }
    function delete_queue() {
        if (is_empty_queue()) {
            error("FRONT called with an empty queue");
        } else {
            front_ptr = tail(front_ptr);
        }
    }
    function print_queue() {
        display(front_ptr);
    }
    function dispatch(m) {
        return m === "insert_queue"
        ? insert_queue
        : m === "delete_queue"
        ? delete_queue
        : m === "is_empty_queue"
        ? is_empty_queue
        : m === "print_queue"
        ? print_queue
        : error(m, "Unknow operation -- DISPATCH");
    }
    return dispatch;
}
function insert_queue(queue, item) {
    return queue("insert_queue")(item);
}
function delete_queue(queue) {
    return queue("delete_queue")();
}
function print_queue(queue) {
    return queue("print_queue")();
}

const q = make_queue();
print_queue(q); // prints: null
insert_queue(q, "a");
print_queue(q); // prints: ["a", null]
insert_queue(q, "b");
print_queue(q); // prints: ["a", ["b", null]]
delete_queue(q);
print_queue(q); // prints: ["b", null]
