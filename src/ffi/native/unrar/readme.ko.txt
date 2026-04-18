                       포터블 UnRAR 버전


   1. 일반

   이 패키지에는 여러 Unix 컴파일러를 위한 프리웨어(freeware) Unrar C++ 소스와
   makefile이 포함되어 있습니다.

   Unrar 소스는 RAR의 하위 집합이며, '#ifndef UNRAR ... #endif'와 같은 블록을
   제거하는 작은 프로그램에 의해 RAR 소스에서 자동으로 생성됩니다. 이러한
   방식은 완벽하지 않으며, 특히 헤더 파일에서 Unrar에 필요하지 않은 일부 RAR
   관련 내용을 찾을 수 있습니다.

   Unrar를 새로운 플랫폼으로 포팅하려면 os.hpp의 '#define LITTLE_ENDIAN'과
   rartypes.hpp의 데이터 유형 정의를 편집해야 할 수 있습니다.

   컴퓨터 아키텍처가 정렬되지 않은 데이터 접근을 허용하지 않는 경우 os.h에서
   ALLOW_NOT_ALIGNED_INT를 정의 해제하고 STRICT_ALIGNMENT_REQUIRED를
   정의해야 합니다.

   UnRAR.vcproj 및 UnRARDll.vcproj는 Microsoft Visual C++용 프로젝트입니다.
   UnRARDll.vcproj를 사용하면 unrar.dll 라이브러리를 빌드할 수 있습니다.


   2. Unrar 바이너리

   www.rarlab.com의 "Downloads" 및 "RAR extras"에 없는 OS용으로 Unrar를
   컴파일한 경우, 컴파일된 실행 파일을 보내주시면 저희 사이트에 게시하겠습니다.


   3. 감사의 글

   이 소스에는 다른 저자가 작성한 코드의 일부가 포함되어 있습니다. 자세한
   내용은 acknow.ko.txt 파일을 참조하십시오.


   4. 법적 관련 사항

   Unrar 소스는 제한 없이 무료로 RAR 압축 파일을 처리하기 위해 모든 소프트웨어에서
   사용할 수 있지만, 독점 기술인 RAR 압축 알고리즘을 재현하는 데 사용할 수는
   없습니다. 별도의 형태 또는 다른 소프트웨어의 일부로 수정된 Unrar 소스를
   배포하는 것은 문서 및 소스 주석에 이 코드가 RAR(WinRAR) 호환 아카이버를
   개발하는 데 사용될 수 없음을 명확하게 명시한 경우에만 허용됩니다.

   더 자세한 라이선스 텍스트는 license.ko.txt에서 확인할 수 있습니다.
