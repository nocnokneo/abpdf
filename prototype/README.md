# Feasibility prototype

`redline_proto.py` is the throwaway script that showed the approach works: a continuous word diff with region and regex filters, the original pages embedded as vector content, highlights, connectors and a linked summary page.
It is a reference for behaviour, not a base for the implementation.

It needs PyMuPDF, which is AGPL licensed, and `git` on the path.
abpdf itself must not depend on either.

    python redline_proto.py OLD.pdf NEW.pdf -o redline.pdf --header 95 --footer 50
