use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandExecutionError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Parse(#[from] shell_words::ParseError),
    #[error("no first argument in command; it may be blank")]
    InvalidArgs,
}


#[derive(Error, Debug)]
pub enum RunActionError {
    #[error(transparent)]
    CommandExecutionError(#[from] CommandExecutionError),
}