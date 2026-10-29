function front_ptr(queue) { return head(queue); }

function rear_ptr(queue) { return tail(queue); }

function set_front_ptr(queue, item) { set_head(queue, item); }

function set_rear_ptr(queue, item) { set_tail(queue, item); }
function make_queue() { return pair(null, null); }

const q = make_queue();
front_ptr(q);

// expected: null
