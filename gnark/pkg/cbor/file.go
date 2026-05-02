// go.mod:
// require github.com/fxamacker/cbor/v2 v2.6.0 // or latest

package cborio

import (
	"fmt"
	"os"

	"github.com/fxamacker/cbor/v2"
)

type InputFile struct {
	U      uint64  `cbor:"U"`
	L      uint64  `cbor:"l"`
	M      uint64  `cbor:"m"`
	S      uint64  `cbor:"s"`
	Rs     uint64  `cbor:"Rs"`
	Values []int64 `cbor:"values"`
}

func NewInputFile(u, l, m, s, rs uint64, values []int64) (*InputFile, error) {
	inf := &InputFile{U: u, L: l, M: m, S: s, Rs: rs, Values: values}
	if err := inf.Validate(); err != nil {
		return nil, err
	}
	return inf, nil
}

func (f *InputFile) Validate() error {
	if uint64(len(f.Values)) != f.U*(1<<f.L) {
		return fmt.Errorf("values length (%d) does not match U (%d) * (1 << l) (%d)", len(f.Values), f.U, 1<<f.L)
	}
	return nil
}

type OutputFile struct {
	Values []int64 `cbor:"values"`
}

func NewOutputFile(values []int64) *OutputFile {
	return &OutputFile{Values: values}
}

var enc, _ = cbor.CanonicalEncOptions().EncMode() // deterministic; use default if you prefer
var dec, _ = cbor.DecOptions{
	MaxArrayElements: 1 << 30, // change here if face error like "cbor: exceeded max number of elements 1048576 for CBOR array"
}.DecMode()

// -------- In-memory encode/decode --------

func (f *InputFile) Marshal() ([]byte, error) {
	if err := f.Validate(); err != nil {
		return nil, err
	}
	return enc.Marshal(f)
}

func UnmarshalInputFile(b []byte) (*InputFile, error) {
	var f InputFile
	if err := dec.Unmarshal(b, &f); err != nil {
		return nil, err
	}
	if err := f.Validate(); err != nil {
		return nil, err
	}
	return &f, nil
}

func (f *OutputFile) Marshal() ([]byte, error) {
	return enc.Marshal(f)
}

func UnmarshalOutputFile(b []byte) (*OutputFile, error) {
	var f OutputFile
	if err := dec.Unmarshal(b, &f); err != nil {
		return nil, err
	}
	return &f, nil
}

// -------- File helpers --------

func (f *InputFile) WriteFile(path string) error {
	b, err := f.Marshal()
	if err != nil {
		return err
	}
	return os.WriteFile(path, b, 0o644)
}

func ReadInputFile(path string) (*InputFile, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return UnmarshalInputFile(b)
}

func (f *OutputFile) WriteFile(path string) error {
	b, err := f.Marshal()
	if err != nil {
		return err
	}
	return os.WriteFile(path, b, 0o644)
}

func ReadOutputFile(path string) (*OutputFile, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return UnmarshalOutputFile(b)
}
