// solution provided by GitHub user clean99
function make_deque() {
    return pair(null, null);
}

function front_ptr(deque) {
    return head(deque);
}

function rear_ptr(deque) {
    return tail(deque);
}

function set_front_ptr(deque, item) {
    set_head(deque, item);
}

function set_rear_ptr(deque, item) {
    set_tail(deque, item);
}

function is_empty_deque(deque) {
    return is_null(front_ptr(deque))
           ? true
  	   : is_null(rear_ptr(deque))
  	   ? true
  	   : false;
}

function is_one_item_deque(deque) {
    return front_ptr(deque) === rear_ptr(deque);
}

function front_insert_deque(deque, item) {
    // use another pair to store a forward pointer
    const new_pair = pair(pair(item, null), null);
    if (is_empty_deque(deque)) {
        set_front_ptr(deque, new_pair);
        set_rear_ptr(deque, new_pair);
    } else {
        set_tail(new_pair, front_ptr(deque));
        // set forward pointer to new_pair
        set_tail(head(front_ptr(deque)), new_pair);
        set_front_ptr(deque, new_pair);
    }
}

function front_delete_deque(deque) {
    if (is_empty_deque(deque)) {
        error(deque, "front_delete_deque called with an empty deque");
    } else if(is_one_item_deque(deque)) {
        set_front_ptr(deque, null);
        set_rear_ptr(deque, null);
        return deque;
    } else {
        set_front_ptr(deque, tail(front_ptr(deque)));
        return deque;
    }
}

function rear_insert_deque(deque, item) {
    if (is_empty_deque(deque)) {
        const new_pair = pair(pair(item, null), null);
        set_front_ptr(deque, new_pair);
        set_rear_ptr(deque, new_pair);
    } else {
        // set new_pair forward pointer to last item
        const new_pair = pair(pair(item, rear_ptr(deque)), null);
        set_tail(rear_ptr(deque), new_pair);
        set_rear_ptr(deque, new_pair);
    }
}

function rear_delete_deque(deque) {
    if (is_empty_deque(deque)) {
        error(deque, "rear_delete_deque called with an empty deque");
    } else if(is_one_item_deque(deque)) {
        set_front_ptr(deque, null);
        set_rear_ptr(deque, null);
        return deque;
    } else {
        // update rear_ptr to last item's forward pointer
        set_rear_ptr(deque, tail(head(rear_ptr(deque))));
        return deque;
    }
}
