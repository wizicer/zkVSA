CORPUS = ../corpus/librispeech/flat
BIN = cargo run --bin dir_pvtsm --

.DEFAULT_GOAL := all

flat%:
	$(BIN) $(CORPUS)/enroll $(CORPUS) --name test --semitones $*.0

all: flat4 flat2

.PHONY: flat% all