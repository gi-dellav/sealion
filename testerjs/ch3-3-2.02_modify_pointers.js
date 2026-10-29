function front_ptr(queue) { return head(queue); }

function rear_ptr(queue) { return tail(queue); }

function set_front_ptr(queue, item) { set_head(queue, item); }

function set_rear_ptr(queue, item) { set_tail(queue, item); }

const q = pair(1, 2);
set_front_ptr(q, 42);	
front_ptr(q);

// expected: 42
