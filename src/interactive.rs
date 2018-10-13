use channel;
use signal_hook;
use std::convert::From;
use std::fmt;
use std::fs;
use std::io::Error as IOErr;
use std::io::{Stdin, Write};
use std::thread;
use termion::screen::AlternateScreen;
use termion::{clear, cursor};

pub fn filter<I>(opts_iter: I, stdin: Stdin, stdout: fs::File) -> Result<String, FilterError>
where
    I: Iterator<Item = (String, f64)>,
{
    // So, this is a massive abstraction leak, but unix signals are icky so it's not really
    // surprising.
    // Because we're popping over to an alternative screen buffer, we need to restore the teriminal
    // when we're done, even if we get sigint / sigterm.
    // This mess is to handle that.
    // Basically, to restore the screen buffer we just need to return from this fn so
    // 'alt' is dropped.
    // We do this by waiting for signals or user-input, and just returning on whichever we see
    // first.
    // That makes 'sigint'/'sigterm' result in us exiting.
    let mut alt = AlternateScreen::from(stdout);
    let signal = notify(&[signal_hook::SIGINT, signal_hook::SIGTERM]);
    let (suser_input, ruser_input) = channel::bounded(0);
    // Wait for a signal or for the user to select a choice.
    write!(alt, "{}{}", clear::All, cursor::Goto(1, 1))?;

    let opts = {
        let mut opts = opts_iter.collect::<Vec<_>>();
        opts.reverse();
        opts
    };

    for i in 0..opts.len() {
        write!(alt, "{}\t{}\t{}\n", opts.len() - i, opts[i].1, opts[i].0)?;
    }
    write!(alt, "> ")?;
    alt.flush()?;
    thread::spawn(move || {
        // Since threads can be messy, do the minimum possible in the thread.
        let mut input = String::new();
        suser_input.send(stdin.read_line(&mut input).map(|_| input));
    });
    select! {
        recv(signal, _msg) =>  {
            return Err(FilterError::NoSelection);
        },
        recv(ruser_input, res) => {
            return res.unwrap()
                .map_err(|_| FilterError::String("unable to read input".to_string()))
                .and_then(|val| {
                    if val.trim().is_empty() {
                        debug!("user input is empty");
                        Err(FilterError::NoSelection)
                    } else {
                        Ok(val)
                    }
                })
                .and_then(|val| {
                    val.trim().parse::<usize>()
                        .map_err(|e| {
                            FilterError::String(format!("unable to parse selection: {}", e))
                        })
                })
                .and_then(|ndx| {
                    if ndx > opts.len() {
                        Err(FilterError::String("index out of bounds".to_string()))
                    } else if ndx > opts.len() {
                        // handle separately from 'opts.get' to avoid underflow panicing
                        Err(FilterError::String("index out of bounds".to_string()))
                    } else {
                        opts.get(opts.len() - ndx).map(|o| o.0.clone())
                            .ok_or(FilterError::String("index out of bounds".to_string()))
                    }
                });
        },
    };
}


fn notify(signals: &[i32]) -> Result<channel::Receiver<i32>, IOErr> {
    let (s, r) = channel::bounded(100);
    let signals = signal_hook::iterator::Signals::new(signals)?;
    thread::spawn(move || {
        for signal in signals.forever() {
            s.send(signal);
        }
    });
    Ok(r)
}

#[derive(Debug)]
pub enum FilterError {
    WriteErr(IOErr),
    String(String),
    NoSelection,
}

impl From<IOErr> for FilterError {
    fn from(e: IOErr) -> Self {
        FilterError::WriteErr(e)
    }
}

impl From<String> for FilterError {
    fn from(e: String) -> Self {
        FilterError::String(e)
    }
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &FilterError::String(ref s) => s.fmt(f),
            &FilterError::NoSelection => Ok(()),
            &FilterError::WriteErr(ref ioe) => ioe.fmt(f),
        }
    }
}
