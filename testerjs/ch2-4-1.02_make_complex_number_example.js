const my_co_num_1 = make_from_real_imag(2.5, -0.5);
const my_co_num_2 = make_from_real_imag(2.5, -0.5);

const result = add_complex(my_co_num_1,
                           mul_complex(my_co_num_2,
                                       my_co_num_2));

imag_part(result);
