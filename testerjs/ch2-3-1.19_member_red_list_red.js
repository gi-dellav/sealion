function member(item, x) {
    return is_null(x)
           ? null
           : item === head(x)
           ? x
           : member(item, tail(x));
}
member("shoes", list("red", "shoes", "blue", "socks"));
// ["shoes", ["blue", ["socks", null]]]

// expected: [ 'shoes', [ 'blue', [ 'socks', null ] ] ]
