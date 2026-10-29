function entry(tree) { return head(tree); }

function left_branch(tree) { return head(tail(tree)); }

function right_branch(tree) { return head(tail(tail(tree))); }

function make_tree(entry, left, right) { 
    return list(entry, left, right);
}
const my_fav_planets =
    make_tree(make_record(4, "Mars"),	
              make_tree(make_record(2, "Venus"), 
                        null,
                        make_tree(make_record(3, "Earth"),
                                  null, null)),
              make_tree(make_record(6, "Saturn"),
                        make_tree(make_record(5, "Jupiter"),
                                  null, null),
                        null));

lookup(3, my_fav_planets);
