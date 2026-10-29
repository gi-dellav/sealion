const m = make_mobile(
              make_branch(20, 
                  make_mobile(make_branch(10, 2),
                              make_branch(4, 5))), 
              make_branch(28, 5));
is_balanced(m);
