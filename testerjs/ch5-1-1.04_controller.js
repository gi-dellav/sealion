function controller(sequence) {	
    return list("controller", sequence);
}
function controller_sequence(controller) {
    return head(tail(controller));
}
