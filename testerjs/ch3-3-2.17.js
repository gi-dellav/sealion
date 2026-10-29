function make_queue() { return pair(null, null); }
function front_ptr(queue) { return head(queue); }

function rear_ptr(queue) { return tail(queue); }

function set_front_ptr(queue, item) { set_head(queue, item); }

function set_rear_ptr(queue, item) { set_tail(queue, item); }
function is_empty_queue(queue) { return is_null(front_ptr(queue)); }
function insert_queue(queue, item) {
    const new_pair = pair(item, null);
    if (is_empty_queue(queue)) {
        set_front_ptr(queue, new_pair);
        set_rear_ptr(queue, new_pair);
    } else {
        set_tail(rear_ptr(queue), new_pair);
        set_rear_ptr(queue, new_pair);
    }
    return queue;
}
function delete_queue(queue) {
    if (is_empty_queue(queue)) {
        error(queue, "delete_queue called with an empty queue");
    } else {
        set_front_ptr(queue, tail(front_ptr(queue)));
        return queue;
    }
}
function print_queue(q) {
    return display(head(q));
}

const q1 = make_queue();
print_queue(q1); // prints: null
insert_queue(q1, "a");
print_queue(q1); // prints: ["a", null]
insert_queue(q1, "b");
print_queue(q1); // prints: ["a", ["b", null]]
delete_queue(q1);
print_queue(q1); // prints: ["b", null]
