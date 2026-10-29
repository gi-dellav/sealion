function attach_tag(type_tag, contents) {
    return pair(type_tag, contents);
}
function type_tag(datum) {
    return is_pair(datum)
           ? head(datum)
           : error(datum, "bad tagged datum -- type_tag");
}
function contents(datum) {
    return is_pair(datum)
           ? tail(datum)
           : error(datum, "bad tagged datum -- contents");
}

const f_1 = list("A", 4);
const my_frequency_1 = 
    attach_tag("frequency_list", f_1);

type_tag(my_frequency_1);

// expected: 'frequency_list'
