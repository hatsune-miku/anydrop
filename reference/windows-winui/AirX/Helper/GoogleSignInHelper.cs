using Newtonsoft.Json;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Sockets;
using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using Windows.Web.Http;
using Windows.Storage.Streams;
using System.Diagnostics;

namespace AnyDrop.Helper
{
    class GoogleSignInHelper
    {
        // client configuration
        const string clientID = "";
        const string clientSecret = "";
        const string authorizationEndpoint = "https://accounts.google.com/o/oauth2/v2/auth";
        const string tokenEndpoint = "https://www.googleapis.com/oauth2/v4/token";
        const string userInfoEndpoint = "https://www.googleapis.com/oauth2/v3/userinfo";

        public static int GetRandomUnusedPort()
        {
            var listener = new TcpListener(IPAddress.Loopback, 0);
            listener.Start();
            var port = ((IPEndPoint)listener.LocalEndpoint).Port;
            listener.Stop();
            return port;
        }

        public async Task TrySignInAsync()
        {
            // Generates state and PKCE values.
            string state = randomDataBase64url(32);
            string code_verifier = randomDataBase64url(32);
            string code_challenge = base64urlencodeNoPadding(sha256(code_verifier));
            const string code_challenge_method = "S256";

            // Creates a redirect URI using an available port on the loopback address.
            string redirectURI = string.Format("http://{0}:{1}/", IPAddress.Loopback, GetRandomUnusedPort());
            Debug.WriteLine("redirect URI: " + redirectURI);

            // Creates an HttpListener to listen for requests on that redirect URI.
            var http = new HttpListener();
            http.Prefixes.Add(redirectURI);
            Debug.WriteLine("Listening..");
            http.Start();

            // Creates the OAuth 2.0 authorization request.
            string authorizationRequest = string.Format("{0}?response_type=code&scope=openid%20profile&redirect_uri={1}&client_id={2}&state={3}&code_challenge={4}&code_challenge_method={5}",
                authorizationEndpoint,
                System.Uri.EscapeDataString(redirectURI),
                clientID,
                state,
                code_challenge,
                code_challenge_method);

            // Opens request in the browser.
            try
            {
                var startInfo = new ProcessStartInfo
                {
                    FileName = authorizationRequest,
                    UseShellExecute = true,
                    CreateNoWindow = true,
                };
                Process.Start(startInfo);
            }
            catch (Exception ex)
            {
                Debug.WriteLine(ex.ToString());
            }

            // Waits for the OAuth authorization response.
            var context = await http.GetContextAsync();

            // Sends an HTTP response to the browser.
            var response = context.Response;

            // Read ms-appx:///Assets/sign-in.html
            var signInHtmlFile = await Windows.Storage.StorageFile.GetFileFromApplicationUriAsync
                (new Uri("ms-appx:///Assets/sign-in.html"));
            string responseString = await Windows.Storage.FileIO.ReadTextAsync(signInHtmlFile);
            var buffer = Encoding.UTF8.GetBytes(responseString);

            response.ContentLength64 = buffer.Length;
            var responseOutput = response.OutputStream;
            Task responseTask = responseOutput.WriteAsync(buffer, 0, buffer.Length)
                .ContinueWith((task) =>
                {
                    responseOutput.Close();
                    http.Stop();
                    Debug.WriteLine("HTTP server stopped.");
                }, TaskScheduler.Default);

            // Checks for errors.
            if (context.Request.QueryString.Get("error") != null)
            {
                Debug.WriteLine(String.Format("OAuth authorization error: {0}.", context.Request.QueryString.Get("error")));
                return;
            }
            if (context.Request.QueryString.Get("code") == null
                || context.Request.QueryString.Get("state") == null)
            {
                Debug.WriteLine("Malformed authorization response. " + context.Request.QueryString);
                return;
            }

            // extracts the code
            var code = context.Request.QueryString.Get("code");
            var incoming_state = context.Request.QueryString.Get("state");

            // Compares the receieved state to the expected value, to ensure that
            // this app made the request which resulted in authorization.
            if (incoming_state != state)
            {
                Debug.WriteLine(String.Format("Received request with invalid state ({0})", incoming_state));
                return;
            }
            Debug.WriteLine("Authorization code: " + code);

            // Starts the code exchange at the Token Endpoint.
            await PerformCodeExchangeAsync(code, code_verifier, redirectURI);
        }

        async Task PerformCodeExchangeAsync(string code, string code_verifier, string redirectURI)
        {
            Debug.WriteLine("Exchanging code for tokens...");

            // builds the request
            string tokenRequestURI = "https://www.googleapis.com/oauth2/v4/token";
            string tokenRequestBody = string.Format("code={0}&redirect_uri={1}&client_id={2}&code_verifier={3}&client_secret={4}&scope=&grant_type=authorization_code",
                code,
                System.Uri.EscapeDataString(redirectURI),
                clientID,
                code_verifier,
                clientSecret);

            // sends the request
            var uri = new Uri(tokenRequestURI);
            var requestMessage = new HttpRequestMessage(HttpMethod.Post, uri);
            var httpClient = new HttpClient();
            var requestContent = new HttpStringContent(tokenRequestBody);
            httpClient.DefaultRequestHeaders.Accept.Add(new("Accept=text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"));
            requestContent.Headers.ContentType = new("application/x-www-form-urlencoded");
            requestMessage.Content = requestContent;

            try
            {
                // gets the response
                var response = await httpClient.SendRequestAsync(requestMessage);
                var responseString = await response.Content.ReadAsStringAsync();
                Debug.WriteLine(responseString);

                // converts to dictionary
                Dictionary<string, string> tokenEndpointDecoded =
                    JsonConvert.DeserializeObject<Dictionary<string, string>>(responseString);

                string access_token = tokenEndpointDecoded["access_token"];
                await TryGetUserInfoAsync(access_token);
            }
            catch (WebException ex)
            {
                if (ex.Status == WebExceptionStatus.ProtocolError)
                {
                    var response = ex.Response as HttpWebResponse;
                    if (response != null)
                    {
                        Debug.WriteLine("HTTP: " + response.StatusCode);
                        using (StreamReader reader = new(response.GetResponseStream()))
                        {
                            // reads response body
                            string responseText = await reader.ReadToEndAsync();
                            Debug.WriteLine(responseText);
                        }
                    }

                }
            }
        }

        async Task TryGetUserInfoAsync(string accessToken)
        {
            Debug.WriteLine("Making API Call to Userinfo...");

            // builds the request
            string userinfoRequestURI = "https://www.googleapis.com/oauth2/v3/userinfo";

            // sends the request
            var uri = new Uri(userinfoRequestURI);
            var requestMessage = new HttpRequestMessage(HttpMethod.Get, uri);
            var httpClient = new HttpClient();
            httpClient.DefaultRequestHeaders.Accept.Add(new("Accept=text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"));
            requestMessage.Headers.Authorization = new("Bearer", accessToken);

            var response = await httpClient.SendRequestAsync(requestMessage);
            var responseString = await response.Content.ReadAsStringAsync();

            Debug.WriteLine(responseString);
        }

        /// <summary>
        /// Returns URI-safe data with a given input length.
        /// </summary>
        /// <param name="length">Input length (nb. output will be longer)</param>
        /// <returns></returns>
        public static string randomDataBase64url(uint length)
        {
            var rng = RandomNumberGenerator.Create();
            byte[] bytes = new byte[length];
            rng.GetBytes(bytes);
            return base64urlencodeNoPadding(bytes);
        }

        public static byte[] sha256(string inputStirng)
        {
            byte[] bytes = Encoding.ASCII.GetBytes(inputStirng);
            var sha256 = SHA256.Create();
            return sha256.ComputeHash(bytes);
        }

        public static string base64urlencodeNoPadding(byte[] buffer)
        {
            string base64 = Convert.ToBase64String(buffer);

            // Converts base64 to base64url.
            base64 = base64.Replace("+", "-");
            base64 = base64.Replace("/", "_");
            // Strips padding.
            base64 = base64.Replace("=", "");

            return base64;
        }
    }
}
