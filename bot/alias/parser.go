package alias

// #cgo LDFLAGS: -L${SRCDIR}/parser/target/release/ -lparser -ldl
// #include "parser/parser.h"
import "C"
import (
	"unsafe"
)

type parseResultKind int

const (
	noMatch parseResultKind = iota
	parseError
	dataAvailable
)

type parseData struct {
	prefix     string
	subCommand string
	args       []string
}

type parseResult struct {
	kind     parseResultKind
	errorMsg string
	data     parseData
}

func parse(text string) parseResult {
	ctext := C.CString(text)

	// Rustで書かれたパーサーを呼び出します
	// なんとなくやりたくなっただけで、Rustでやる意味あるんですかと
	// 聞かれたら、無いですと答えます。
	res := C.parse(ctext)

	C.free(unsafe.Pointer(ctext))

	var (
		ok             = bool(res.ok)
		data_available = bool(res.data_available)
		result         parseResult
	)

	switch {
	case ok && !data_available:
		result.kind = noMatch

	case !ok && !data_available:
		result.kind = parseError
		result.errorMsg = C.GoString(res.error_msg)

	case ok && data_available:
		result = parseResult{
			kind:     dataAvailable,
			errorMsg: "",
			data: parseData{
				prefix:     safeGoString(res.data.prefix),
				subCommand: safeGoString(res.data.sub_command),
				args:       decodeStringSlice(res.data.args, res.data.args_length),
			},
		}
	}

	C.free_parse_result(res)
	return result
}

func safeGoString(ptr *C.char) (result string) {
	if ptr != nil {
		result = C.GoString(ptr)
	}
	return
}

func decodeStringSlice(ptr unsafe.Pointer, len C.uint) []string {
	result := make([]string, len)

	for i := 0; i < int(len); i++ {
		result[i] = safeGoString(C.args_get_at(ptr, C.ulong(i)))
	}

	return result
}
