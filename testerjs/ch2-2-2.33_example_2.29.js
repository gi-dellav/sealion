const m = make_mobile(
              make_branch(10,
                  make_mobile(make_branch(10, 2), 
                      make_branch(4, 5))), 
              make_branch(10, 23));      
total_weight(m);
