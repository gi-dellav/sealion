function make_record(key, data) {	  
    return pair(key, data);
}
function key(record) {    
    return head(record);
}
function data(record) {
    return tail(record);
}
