function make_agenda() { return list(0); }

function current_time(agenda) { return head(agenda); }

function set_current_time(agenda, time) {
   set_head(agenda, time);
}
function segments(agenda) { return tail(agenda); }

function set_segments(agenda, segs) {
   set_tail(agenda, segs);
}
function first_segment(agenda) { return head(segments(agenda)); }

function rest_segments(agenda) { return tail(segments(agenda)); }
function is_empty_agenda(agenda) {
   return is_null(segments(agenda));
}
function make_time_segment(time, queue) {
   return pair(time, queue);
}
function segment_time(s) { return head(s); }

function segment_queue(s) { return tail(s); }
function front_ptr(queue) { return head(queue); }

function rear_ptr(queue) { return tail(queue); }

function set_front_ptr(queue, item) { set_head(queue, item); }

function set_rear_ptr(queue, item) { set_tail(queue, item); }
function is_empty_queue(queue) { return is_null(front_ptr(queue)); }
function front_queue(queue) {
    return is_empty_queue(queue)
           ? error(queue, "front_queue called with an empty queue")
           : head(front_ptr(queue));
}
function first_agenda_item(agenda) {
    if (is_empty_agenda(agenda)) {
        error("agenda is empty -- first_agenda_item");
    } else {
        const first_seg = first_segment(agenda);
        set_current_time(agenda, segment_time(first_seg));
        return front_queue(segment_queue(first_seg));
    }
}
