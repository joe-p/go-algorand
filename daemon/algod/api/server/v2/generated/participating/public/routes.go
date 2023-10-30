// Package public provides primitives to interact with the openapi HTTP API.
//
// Code generated by github.com/algorand/oapi-codegen DO NOT EDIT.
package public

import (
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"fmt"
	"net/http"
	"net/url"
	"path"
	"strings"

	. "github.com/algorand/go-algorand/daemon/algod/api/server/v2/generated/model"
	"github.com/algorand/oapi-codegen/pkg/runtime"
	"github.com/getkin/kin-openapi/openapi3"
	"github.com/labstack/echo/v4"
)

// ServerInterface represents all server handlers.
type ServerInterface interface {
	// Get a list of unconfirmed transactions currently in the transaction pool by address.
	// (GET /v2/accounts/{address}/transactions/pending)
	GetPendingTransactionsByAddress(ctx echo.Context, address string, params GetPendingTransactionsByAddressParams) error
	// Broadcasts a raw transaction or transaction group to the network.
	// (POST /v2/transactions)
	RawTransaction(ctx echo.Context) error
	// Get a list of unconfirmed transactions currently in the transaction pool.
	// (GET /v2/transactions/pending)
	GetPendingTransactions(ctx echo.Context, params GetPendingTransactionsParams) error
	// Get a specific pending transaction.
	// (GET /v2/transactions/pending/{txid})
	PendingTransactionInformation(ctx echo.Context, txid string, params PendingTransactionInformationParams) error
}

// ServerInterfaceWrapper converts echo contexts to parameters.
type ServerInterfaceWrapper struct {
	Handler ServerInterface
}

// GetPendingTransactionsByAddress converts echo context to params.
func (w *ServerInterfaceWrapper) GetPendingTransactionsByAddress(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "address" -------------
	var address string

	err = runtime.BindStyledParameterWithLocation("simple", false, "address", runtime.ParamLocationPath, ctx.Param("address"), &address)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter address: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params GetPendingTransactionsByAddressParams
	// ------------- Optional query parameter "max" -------------

	err = runtime.BindQueryParameter("form", true, false, "max", ctx.QueryParams(), &params.Max)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter max: %s", err))
	}

	// ------------- Optional query parameter "format" -------------

	err = runtime.BindQueryParameter("form", true, false, "format", ctx.QueryParams(), &params.Format)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter format: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetPendingTransactionsByAddress(ctx, address, params)
	return err
}

// RawTransaction converts echo context to params.
func (w *ServerInterfaceWrapper) RawTransaction(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.RawTransaction(ctx)
	return err
}

// GetPendingTransactions converts echo context to params.
func (w *ServerInterfaceWrapper) GetPendingTransactions(ctx echo.Context) error {
	var err error

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params GetPendingTransactionsParams
	// ------------- Optional query parameter "max" -------------

	err = runtime.BindQueryParameter("form", true, false, "max", ctx.QueryParams(), &params.Max)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter max: %s", err))
	}

	// ------------- Optional query parameter "format" -------------

	err = runtime.BindQueryParameter("form", true, false, "format", ctx.QueryParams(), &params.Format)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter format: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.GetPendingTransactions(ctx, params)
	return err
}

// PendingTransactionInformation converts echo context to params.
func (w *ServerInterfaceWrapper) PendingTransactionInformation(ctx echo.Context) error {
	var err error
	// ------------- Path parameter "txid" -------------
	var txid string

	err = runtime.BindStyledParameterWithLocation("simple", false, "txid", runtime.ParamLocationPath, ctx.Param("txid"), &txid)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter txid: %s", err))
	}

	ctx.Set(Api_keyScopes, []string{""})

	// Parameter object where we will unmarshal all parameters from the context
	var params PendingTransactionInformationParams
	// ------------- Optional query parameter "format" -------------

	err = runtime.BindQueryParameter("form", true, false, "format", ctx.QueryParams(), &params.Format)
	if err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, fmt.Sprintf("Invalid format for parameter format: %s", err))
	}

	// Invoke the callback with all the unmarshalled arguments
	err = w.Handler.PendingTransactionInformation(ctx, txid, params)
	return err
}

// This is a simple interface which specifies echo.Route addition functions which
// are present on both echo.Echo and echo.Group, since we want to allow using
// either of them for path registration
type EchoRouter interface {
	CONNECT(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	DELETE(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	GET(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	HEAD(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	OPTIONS(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	PATCH(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	POST(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	PUT(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
	TRACE(path string, h echo.HandlerFunc, m ...echo.MiddlewareFunc) *echo.Route
}

// RegisterHandlers adds each server route to the EchoRouter.
func RegisterHandlers(router EchoRouter, si ServerInterface, m ...echo.MiddlewareFunc) {
	RegisterHandlersWithBaseURL(router, si, "", m...)
}

// Registers handlers, and prepends BaseURL to the paths, so that the paths
// can be served under a prefix.
func RegisterHandlersWithBaseURL(router EchoRouter, si ServerInterface, baseURL string, m ...echo.MiddlewareFunc) {

	wrapper := ServerInterfaceWrapper{
		Handler: si,
	}

	router.GET(baseURL+"/v2/accounts/:address/transactions/pending", wrapper.GetPendingTransactionsByAddress, m...)
	router.POST(baseURL+"/v2/transactions", wrapper.RawTransaction, m...)
	router.GET(baseURL+"/v2/transactions/pending", wrapper.GetPendingTransactions, m...)
	router.GET(baseURL+"/v2/transactions/pending/:txid", wrapper.PendingTransactionInformation, m...)

}

// Base64 encoded, gzipped, json marshaled Swagger object
var swaggerSpec = []string{

	"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",
	"iQvcTJog0UeHx5OP/z8AAP//jpo78c4TAQA=",
}

// GetSwagger returns the content of the embedded swagger specification file
// or error if failed to decode
func decodeSpec() ([]byte, error) {
	zipped, err := base64.StdEncoding.DecodeString(strings.Join(swaggerSpec, ""))
	if err != nil {
		return nil, fmt.Errorf("error base64 decoding spec: %s", err)
	}
	zr, err := gzip.NewReader(bytes.NewReader(zipped))
	if err != nil {
		return nil, fmt.Errorf("error decompressing spec: %s", err)
	}
	var buf bytes.Buffer
	_, err = buf.ReadFrom(zr)
	if err != nil {
		return nil, fmt.Errorf("error decompressing spec: %s", err)
	}

	return buf.Bytes(), nil
}

var rawSpec = decodeSpecCached()

// a naive cached of a decoded swagger spec
func decodeSpecCached() func() ([]byte, error) {
	data, err := decodeSpec()
	return func() ([]byte, error) {
		return data, err
	}
}

// Constructs a synthetic filesystem for resolving external references when loading openapi specifications.
func PathToRawSpec(pathToFile string) map[string]func() ([]byte, error) {
	var res = make(map[string]func() ([]byte, error))
	if len(pathToFile) > 0 {
		res[pathToFile] = rawSpec
	}

	return res
}

// GetSwagger returns the Swagger specification corresponding to the generated code
// in this file. The external references of Swagger specification are resolved.
// The logic of resolving external references is tightly connected to "import-mapping" feature.
// Externally referenced files must be embedded in the corresponding golang packages.
// Urls can be supported but this task was out of the scope.
func GetSwagger() (swagger *openapi3.T, err error) {
	var resolvePath = PathToRawSpec("")

	loader := openapi3.NewLoader()
	loader.IsExternalRefsAllowed = true
	loader.ReadFromURIFunc = func(loader *openapi3.Loader, url *url.URL) ([]byte, error) {
		var pathToFile = url.String()
		pathToFile = path.Clean(pathToFile)
		getSpec, ok := resolvePath[pathToFile]
		if !ok {
			err1 := fmt.Errorf("path not found: %s", pathToFile)
			return nil, err1
		}
		return getSpec()
	}
	var specData []byte
	specData, err = rawSpec()
	if err != nil {
		return
	}
	swagger, err = loader.LoadFromData(specData)
	if err != nil {
		return
	}
	return
}
