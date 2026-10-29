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
