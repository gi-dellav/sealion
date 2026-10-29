function make_time_segment(time, queue) {
   return pair(time, queue);
}
function segment_time(s) { return head(s); }

function segment_queue(s) { return tail(s); }
