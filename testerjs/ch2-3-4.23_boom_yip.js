function make_leaf(symbol, weight) {
    return list("leaf", symbol, weight);
}
function is_leaf(object) {
    return head(object) === "leaf";
}
function symbol_leaf(x) { return head(tail(x)); }

function weight_leaf(x) { return head(tail(tail(x))); }
function left_branch(tree) { return head(tail(tree)); }

function right_branch(tree) { return head(tail(tail(tree))); }

function symbols(tree) {
    return is_leaf(tree)
           ? list(symbol_leaf(tree))
           : head(tail(tail(tail(tree))));
}
function weight(tree) {
    return is_leaf(tree)
           ? weight_leaf(tree)
           : head(tail(tail(tail(tail(tree)))));
}
function make_code_tree(left, right) {
    return list("code_tree", left, right,
                append(symbols(left), symbols(right)),
                weight(left) + weight(right));
}
function adjoin_set(x, set) {
    return is_null(set)
           ? list(x)
           : weight(x) < weight(head(set))
           ? pair(x, set)
           : pair(head(set), adjoin_set(x, tail(set)));
}
function make_leaf_set(pairs) {
    if (is_null(pairs)) {
        return null;
    } else {
        const first_pair = head(pairs);
        return adjoin_set(
                   make_leaf(head(first_pair),        // symbol
                             head(tail(first_pair))), // frequency
                   make_leaf_set(tail(pairs)));
    }
}
// successive_merge function to be written by student
function generate_huffman_tree(pairs) {
    return successive_merge(make_leaf_set(pairs));
}
function successive_merge(leaves) {
    return length(leaves) === 1
           ? head(leaves)
           : successive_merge(
                 adjoin_set(
                     make_code_tree(head(leaves),
                                    head(tail(leaves))),
                     tail(tail(leaves))));
}
function encode_symbol(symbol, tree) {
    function contains_symbol(symbol, current_tree) {
        return ! is_null(member(symbol, symbols(current_tree)));
    }
    if (is_leaf(tree)) {
        return null;
    } else {
        const left_tree = left_branch(tree);
	const right_tree = right_branch(tree);
	return contains_symbol(symbol, left_tree)
               ? pair(0, encode_symbol(symbol, left_tree))
               : contains_symbol(symbol, right_tree)
               ? pair(1, encode_symbol(symbol, right_tree))
               : error("symbol not found -- encode_symbol");
    }
}
// encode_symbol function to be written by students
function encode(message, tree) {
    return is_null(message)
           ? null
           : append(encode_symbol(head(message), tree),
                    encode(tail(message), tree));
}
const lyrics_frequencies = 	  
    list(list("A", 2),
         list("NA", 16),
         list("BOOM", 1),
         list("SHA", 3),
         list("GET", 2),
         list("YIP", 9),
         list("JOB", 2),
         list("WAH", 2));
const lyrics_tree = generate_huffman_tree(lyrics_frequencies);
const lyrics = list(
    'GET', 'A', 'JOB',
    'SHA', 'NA', 'NA', 'NA', 'NA', 'NA', 'NA', 'NA', 'NA',
    'GET', 'A', 'JOB', 'SHA', 'NA', 'NA', 'NA', 'NA', 'NA', 
    'NA', 'NA', 'NA', 'WAH', 'YIP', 'YIP', 'YIP', 'YIP',
    'YIP', 'YIP', 'YIP', 'YIP', 'YIP', 'SHA', 'BOOM'
   );
 
length(encode(lyrics, lyrics_tree));
// 84

// expected: 84
