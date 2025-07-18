use crate::document::Document;
use anyhow::Result;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Voice { pub id: String, pub name: String }

pub trait Audio {
    fn voices(&self) -> Result<Vec<Voice>>;
    fn configure(&mut self, voice: &str, rate: i32) -> Result<()>;
    fn speak(&mut self, text: &str) -> Result<()>;
    fn done(&self) -> Result<bool>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State { Stopped, Playing, Paused, Finished }

pub struct Player<A: Audio> {
    pub audio: A,
    pub state: State,
    pub cursor: usize,
    doc: Option<Arc<Document>>,
    voice: String,
    rate: i32,
}

impl<A: Audio> Player<A> {
    pub fn new(audio: A) -> Self { Self { audio, state: State::Stopped, cursor: 0, doc: None, voice: String::new(), rate: 0 } }
    pub fn start(&mut self, doc: Arc<Document>, cursor: usize, voice: String, rate: i32) -> Result<()> {
        self.stop()?;
        self.cursor = cursor.min(doc.chunks.len()-1);
        self.doc = Some(doc);
        self.voice = voice;
        self.rate = rate;
        self.audio.configure(&self.voice, self.rate)?;
        self.speak_current()?;
        self.state = State::Playing;
        Ok(())
    }
    fn speak_current(&mut self) -> Result<()> { self.audio.speak(&self.doc.as_ref().unwrap().chunks[self.cursor].text) }
    pub fn pause(&mut self) -> Result<()> {
        if self.state == State::Playing { self.audio.pause()?; self.state = State::Paused; }
        Ok(())
    }
    pub fn resume(&mut self) -> Result<()> {
        if self.state == State::Paused { self.audio.resume()?; self.state = State::Playing; }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        self.audio.stop()?;
        self.state = State::Stopped;
        Ok(())
    }
    pub fn tick(&mut self) -> Result<bool> {
        if self.state != State::Playing || !self.audio.done()? { return Ok(false); }
        if self.cursor+1 >= self.doc.as_ref().unwrap().chunks.len() { self.state = State::Finished; }
        else { self.cursor += 1; self.speak_current()?; }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Silent { spoken: Vec<String>, completed: bool, paused: bool }
    impl Audio for Silent {
        fn voices(&self)->Result<Vec<Voice>> { Ok(vec![]) }
        fn configure(&mut self,_:&str,_:i32)->Result<()> { Ok(()) }
        fn speak(&mut self,t:&str)->Result<()> { self.spoken.push(t.into()); self.completed=false; Ok(()) }
        fn done(&self)->Result<bool> { Ok(self.completed) }
        fn pause(&mut self)->Result<()> { self.paused=true; Ok(()) }
        fn resume(&mut self)->Result<()> { self.paused=false; Ok(()) }
        fn stop(&mut self)->Result<()> { self.paused=false; Ok(()) }
    }
    #[test]
    fn pause_stop_seek_and_finish_without_sound() {
        let doc = Arc::new(Document::from_text("".into(), "One. Two. Three.".into()).unwrap());
        let mut p = Player::new(Silent::default());
        p.start(doc.clone(),0,"".into(),0).unwrap();
        assert!(!p.tick().unwrap());
        p.pause().unwrap(); p.audio.completed=true;
        assert!(!p.tick().unwrap()); assert_eq!(p.cursor,0);
        p.resume().unwrap(); assert!(p.tick().unwrap()); assert_eq!(p.cursor,1);
        p.stop().unwrap(); p.audio.completed=true; assert!(!p.tick().unwrap());
        p.start(doc,usize::MAX,"".into(),0).unwrap(); assert_eq!(p.cursor,2);
        p.audio.completed=true; p.tick().unwrap(); assert_eq!(p.state,State::Finished);
        assert_eq!(p.audio.spoken,vec!["One.","Two.","Three."]);
    }
}
