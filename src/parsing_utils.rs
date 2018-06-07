use nom::{Needed, not_line_ending, types::CompleteByteSlice};

macro_rules! parse_as_complete (
	($i:expr, $submac:ident!( $($args:tt)* )) => (
		{
			use nom::Context;

			fn convert_context<E>(context: Context<CompleteByteSlice, E>) -> Context<&[u8], E> {
				let (input, err) = match context {
					Context::Code(CompleteByteSlice(input), err) => (input, err)
				};
				Context::Code(input, err)
			}

			let input = CompleteByteSlice($i);
			match $submac!(input, $($args)*) {
		        Err(err) => match err {
		            Err::Incomplete(needed) => Err(Err::Incomplete(needed)),
		            Err::Error(context) => Err(Err::Error(convert_context(context))),
		            Err::Failure(context) => Err(Err::Failure(convert_context(context)))
		        },
		        Ok((rest, output)) => Ok((rest.0, output.0))
			}
		}
	);
	($i:expr, $f:expr) => (
		parse_as_complete!($i, call!($f));
	);
);

named!(
	pub file_name<Vec<u8>>,
	alt!(quoted_name | map!(parse_as_complete!(not_line_ending), |slice| (if slice.ends_with(&b"\t"[..]) { &slice[..slice.len() - 1] } else { slice }).into()))
);

named!(
	pub quoted_name<Vec<u8>>,
	delimited!(tag!(b"\""), unescape, tag!(b"\""))
);

named!(
	unescape<Vec<u8>>,
	escaped_transform!(
		not_quote_or_backslash,
		'\\',
		alt!(
			one_of!(r#""\"#) => { |ch| vec![ch as u8] } |
			tag!("a") => { |_| vec![b'\x07'] } |
			tag!("b") => { |_| vec![b'\x08'] } |
			tag!("n") => { |_| vec![b'\n'] } |
			tag!("r") => { |_| vec![b'\r'] } |
			tag!("t") => { |_| vec![b'\t'] } |
			tag!("v") => { |_| vec![b'\x0B'] } |
			octal_escape => { |byte| vec![byte] }
	   )
	)
);

named!(not_quote_or_backslash, is_not!(r#""\"#));

named!(
	octal_escape<u8>,
	do_parse!(
		first_digit: one_of!("0123") >>
		second_digit: one_of!("01234567") >>
		third_digit: one_of!("01234567") >>
		(u8::from_str_radix(&vec![first_digit, second_digit, third_digit].into_iter().collect::<String>(), 8).unwrap())
	)
);

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_unescape() {
		let escaped_data = br#""\320\241\321\202\321\200\320\260\320\275\320\275\321\213\320\271 \321\204\320\260\320\271\320\273.txt""#;
		assert_eq!("Странный файл.txt".as_bytes(), &*file_name(&escaped_data[..]).unwrap().1);
	}
}